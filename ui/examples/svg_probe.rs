//! Exercise native SVG rendering without opening any application window.
use native_nsis_ui::svg::SvgLogo;
use std::{fs, path::PathBuf, time::Instant};
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        assert!(CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) >= 0);
    }
    let output = PathBuf::from(std::env::args().nth(1).expect("pass an output directory"));
    fs::create_dir_all(&output)?;
    let source = match std::env::args().nth(2) {
        Some(path) => fs::read(path)?,
        None => include_bytes!("logo.svg").to_vec(),
    };
    let start = Instant::now();
    let mut logo = SvgLogo::new(&source).map_err(|e| std::io::Error::other(e.to_string()))?;
    let init_ms = start.elapsed().as_secs_f64() * 1000.0;
    let mut samples = Vec::new();
    for size in [60, 75, 90, 105, 120, 135, 63, 150, 180, 240] {
        let start = Instant::now();
        logo.render(size)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let render_ms = start.elapsed().as_secs_f64() * 1000.0;
        let pixels = logo.pixels().expect("rendered bitmap");
        assert_eq!(pixels.len(), size as usize * size as usize * 4);
        assert!(pixels.chunks_exact(4).any(|p| p[3] == 0));
        assert!(pixels.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255));
        assert!(pixels.chunks_exact(4).any(|p| p[3] == 255));
        fs::write(output.join(format!("svg-{size}.bgra")), pixels)?;
        let pointer = pixels.as_ptr();
        let start = Instant::now();
        logo.render(size)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let cached_ms = start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(
            logo.pixels().unwrap().as_ptr(),
            pointer,
            "same-DPI painting reuses the bitmap"
        );
        samples.push(format!(
            "{{\"pixels\":{size},\"render_ms\":{render_ms:.4},\"cached_ms\":{cached_ms:.4}}}"
        ));
    }
    assert!(logo.render(0).is_err());
    drop(logo);
    // Exercise resource recreation separately from real device-loss injection.
    let mut replacement =
        SvgLogo::new(&source).map_err(|e| std::io::Error::other(e.to_string()))?;
    replacement
        .render(60)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    assert_eq!(replacement.pixels().unwrap().len(), 60 * 60 * 4);
    drop(replacement);
    unsafe {
        CoUninitialize();
    }
    let json = format!(
        "{{\"backend\":\"Direct2D SVG / D3D11 WARP\",\"init_ms\":{init_ms:.4},\"samples\":[{}]}}",
        samples.join(",")
    );
    fs::write(output.join("svg-render-metrics.json"), &json)?;
    println!("{json}");
    Ok(())
}
