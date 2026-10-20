//! Windows-native SVG -> cached premultiplied BGRA -> existing GDI compositor.
//! WARP avoids GPU/driver requirements; only the small logo is rasterized.
use std::{
    mem::size_of,
    ptr::{copy_nonoverlapping, null_mut},
};
use windows::{
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, HMODULE},
        Graphics::{
            Direct2D::{Common::*, *},
            Direct3D::D3D_DRIVER_TYPE_WARP,
            Direct3D11::{D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice},
            Dxgi::{Common::DXGI_FORMAT_B8G8R8A8_UNORM, IDXGIDevice},
        },
        UI::Shell::SHCreateMemStream,
    },
    core::{Error, Interface, Result, w},
};
use windows_sys::Win32::Graphics::Gdi::*;

pub struct SvgLogo {
    context: ID2D1DeviceContext5,
    document: ID2D1SvgDocument,
    cache: Option<Bitmap>,
}

impl SvgLogo {
    pub fn new(source: &[u8]) -> Result<Self> {
        unsafe {
            let mut d3d = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_WARP,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d),
                None,
                None,
            )?;
            let dxgi: IDXGIDevice = d3d.ok_or_else(|| Error::from_hresult(E_FAIL))?.cast()?;
            let device = D2D1CreateDevice(&dxgi, None)?;
            let context: ID2D1DeviceContext5 = device
                .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?
                .cast()?;
            let stream =
                SHCreateMemStream(Some(source)).ok_or_else(|| Error::from_hresult(E_FAIL))?;
            let document = context.CreateSvgDocument(
                &stream,
                D2D_SIZE_F {
                    width: 60.0,
                    height: 60.0,
                },
            )?;
            // Fit the supplied logo's viewBox into the rendering viewport.
            // Retain its viewBox and preserveAspectRatio settings.
            let root = document.GetRoot()?;
            root.SetAttributeValue3(w!("width"), D2D1_SVG_ATTRIBUTE_STRING_TYPE_SVG, w!("100%"))?;
            root.SetAttributeValue3(w!("height"), D2D1_SVG_ATTRIBUTE_STRING_TYPE_SVG, w!("100%"))?;
            context.SetDpi(96.0, 96.0);
            Ok(Self {
                context,
                document,
                cache: None,
            })
        }
    }

    pub fn render(&mut self, pixels: u32) -> Result<()> {
        if pixels == 0 || pixels > 4096 {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        if self
            .cache
            .as_ref()
            .is_some_and(|cache| cache.size == pixels)
        {
            return Ok(());
        }
        unsafe {
            let size = D2D_SIZE_U {
                width: pixels,
                height: pixels,
            };
            let properties = |options| D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: options,
                ..Default::default()
            };
            let target = self.context.CreateBitmap(
                size,
                None,
                0,
                &properties(D2D1_BITMAP_OPTIONS_TARGET),
            )?;
            self.document.SetViewportSize(D2D_SIZE_F {
                width: pixels as f32,
                height: pixels as f32,
            })?;
            self.context.SetTarget(&target);
            self.context.BeginDraw();
            self.context.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));
            self.context.DrawSvgDocument(&self.document);
            let result = self.context.EndDraw(None, None);
            self.context.SetTarget(None);
            result?;
            let staging = self.context.CreateBitmap(
                size,
                None,
                0,
                &properties(D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW),
            )?;
            staging.CopyFromBitmap(None, &target, None)?;
            let mapped = staging.Map(D2D1_MAP_OPTIONS_READ)?;
            let stride = pixels as usize * 4;
            let mut bgra = vec![0u8; stride * pixels as usize];
            for row in 0..pixels as usize {
                copy_nonoverlapping(
                    mapped.bits.add(row * mapped.pitch as usize),
                    bgra.as_mut_ptr().add(row * stride),
                    stride,
                );
            }
            staging.Unmap()?;
            self.cache = Some(Bitmap::new(pixels, bgra)?);
        }
        Ok(())
    }

    pub fn pixels(&self) -> Option<&[u8]> {
        self.cache.as_ref().map(|cache| cache.bgra.as_slice())
    }

    /// # Safety
    /// `target` must be a valid DC owned by the calling UI thread.
    pub unsafe fn paint(&self, target: HDC, x: i32, y: i32) -> bool {
        self.cache.as_ref().is_some_and(|cache| unsafe {
            GdiAlphaBlend(
                target,
                x,
                y,
                cache.size as i32,
                cache.size as i32,
                cache.dc,
                0,
                0,
                cache.size as i32,
                cache.size as i32,
                BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                },
            ) != 0
        })
    }
}

struct Bitmap {
    size: u32,
    bgra: Vec<u8>,
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
}

impl Bitmap {
    unsafe fn new(size: u32, bgra: Vec<u8>) -> Result<Self> {
        unsafe {
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: size as i32,
                    biHeight: -(size as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = null_mut();
            let bitmap =
                CreateDIBSection(null_mut(), &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() {
                return Err(Error::from_win32());
            }
            let dc = CreateCompatibleDC(null_mut());
            if dc.is_null() {
                DeleteObject(bitmap);
                return Err(Error::from_win32());
            }
            copy_nonoverlapping(bgra.as_ptr(), bits.cast(), bgra.len());
            let previous = SelectObject(dc, bitmap);
            Ok(Self {
                size,
                bgra,
                dc,
                bitmap,
                previous,
            })
        }
    }
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize,
    };

    #[test]
    fn viewbox_fits_without_cropping_or_distorting_non_square_logos() {
        unsafe {
            assert!(CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) >= 0);
        }
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe {
                    CoUninitialize();
                }
            }
        }
        let _apartment = Apartment;
        let mut previous = None;
        for dimensions in ["300 150", "800 400"] {
            let (width, height) = dimensions.split_once(' ').unwrap();
            let source = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 200 100"><rect width="200" height="100" fill="white"/></svg>"#
            );
            let mut logo = SvgLogo::new(source.as_bytes()).unwrap();
            logo.render(64).unwrap();
            let pixels = logo.pixels().unwrap();
            let visible: Vec<_> = pixels
                .chunks_exact(4)
                .enumerate()
                .filter(|(_, pixel)| pixel[3] > 0)
                .map(|(index, _)| (index % 64, index / 64))
                .collect();
            assert_eq!(visible.iter().map(|p| p.0).min(), Some(0));
            assert_eq!(visible.iter().map(|p| p.0).max(), Some(63));
            assert_eq!(visible.iter().map(|p| p.1).min(), Some(16));
            assert_eq!(visible.iter().map(|p| p.1).max(), Some(47));
            if let Some(previous) = previous {
                assert_eq!(previous, pixels);
            }
            previous = Some(pixels.to_vec());
        }
    }
}
