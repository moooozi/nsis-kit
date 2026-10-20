//! Per-process product configuration supplied by the consuming NSIS script.
use super::*;

#[derive(Clone)]
pub(super) struct Branding {
    pub name: String,
    pub description: String,
    // Preview builds validate the same configuration ABI but never launch apps.
    #[cfg_attr(not(feature = "installer"), allow(dead_code))]
    pub executable: String,
    pub installed_message: String,
    pub required_kib: Option<u64>,
    pub maintenance: Maintenance,
    #[cfg_attr(not(feature = "native-svg"), allow(dead_code))]
    pub logo: Vec<u8>,
}

impl Default for Branding {
    fn default() -> Self {
        Self {
            name: "Example App".into(),
            description: "Install this application for all users.".into(),
            executable: "example.exe".into(),
            installed_message: "The application has been installed.".into(),
            required_kib: None,
            maintenance: Maintenance::default(),
            logo: Vec::new(),
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct Maintenance {
    pub action: Replacement,
    pub installed: String,
    pub target: String,
}

#[derive(Clone, Copy, Default)]
pub(super) enum Replacement {
    Upgrade,
    #[default]
    Reinstall,
    Downgrade,
}

impl Replacement {
    pub fn label(self) -> &'static str {
        match self {
            Self::Upgrade => "&Upgrade",
            Self::Reinstall => "&Reinstall",
            Self::Downgrade => "&Downgrade",
        }
    }
    pub fn progress(self) -> &'static str {
        match self {
            Self::Upgrade => "Upgrading",
            Self::Reinstall => "Reinstalling",
            Self::Downgrade => "Downgrading",
        }
    }
    pub fn completed(self) -> &'static str {
        match self {
            Self::Upgrade => "upgraded",
            Self::Reinstall => "reinstalled",
            Self::Downgrade => "downgraded",
        }
    }
}

impl Maintenance {
    pub fn description(&self, product: &str) -> String {
        let versions = if self.installed.is_empty() {
            String::new()
        } else {
            format!(
                "Installed version: {}\nSetup version: {}\n\n",
                self.installed, self.target
            )
        };
        format!(
            "{versions}Close all running instances of {product} before {} or uninstalling.",
            self.action.progress().to_lowercase()
        )
    }
}

/// NSIS supplies the version-based action; the DLL only presents it.
/// Input: upgrade/reinstall/downgrade, installed version, setup version.
/// # Safety
/// Called only through the Unicode NSIS stack ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ConfigureMaintenance(
    _: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size <= 0 {
        return;
    }
    let mode = pop(stack, size as usize);
    let installed = pop(stack, size as usize);
    let target = pop(stack, size as usize);
    let action = match mode.as_str() {
        "upgrade" => Replacement::Upgrade,
        "reinstall" => Replacement::Reinstall,
        "downgrade" => Replacement::Downgrade,
        _ => {
            push(
                stack,
                size as usize,
                "Invalid maintenance presentation action.",
            );
            return;
        }
    };
    BRANDING.with(|branding| {
        branding.borrow_mut().maintenance = Maintenance {
            action,
            installed,
            target,
        }
    });
    push(stack, size as usize, "");
}

thread_local! {
    static BRANDING: RefCell<Branding> = RefCell::new(Branding::default());
}

pub(super) fn current() -> Branding {
    BRANDING.with(|branding| branding.borrow().clone())
}

fn executable_name(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.chars().any(|c| matches!(c, '/' | '\\' | ':' | '"'))
}

/// Configure name, description, executable basename, success text and SVG path.
/// Returns an empty string on success. Each NSIS process configures its own DLL.
/// # Safety
/// Called only through the Unicode NSIS stack ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Configure(
    _: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size <= 0 {
        return;
    }
    let name = pop(stack, size as usize);
    let description = pop(stack, size as usize);
    let executable = pop(stack, size as usize);
    let installed_message = pop(stack, size as usize);
    let path = pop(stack, size as usize);
    let result = (|| {
        if name.is_empty() || !executable_name(&executable) {
            return Err("Product name and executable basename are required.".into());
        }
        let logo = if path.is_empty() {
            Vec::new()
        } else {
            std::fs::read(path).map_err(|e| format!("Cannot read product logo: {e}"))?
        };
        BRANDING.with(|branding| {
            *branding.borrow_mut() = Branding {
                name,
                description,
                executable,
                installed_message,
                required_kib: None,
                maintenance: Maintenance::default(),
                logo,
            };
        });
        Ok::<_, String>(())
    })();
    push(stack, size as usize, &result.err().unwrap_or_default());
}

pub(super) fn required_space_text(kib: Option<u64>) -> String {
    match kib {
        Some(value) if value >= 1024 => format!(
            "At least {} MB of disk space is required.",
            value.div_ceil(1024)
        ),
        Some(value) => format!("At least {value} KB of disk space is required."),
        None => "Required disk space is unavailable.".into(),
    }
}

/// Set the NSIS package's disk-space estimate, in KiB, before opening the UI.
/// # Safety
/// Called only through the Unicode NSIS stack ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn RequiredSpace(
    _: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size <= 0 {
        return;
    }
    let result = pop(stack, size as usize).parse::<u64>();
    let status = match result {
        Ok(value) => {
            BRANDING.with(|branding| branding.borrow_mut().required_kib = Some(value));
            ""
        }
        Err(_) => "Invalid required disk space.",
    };
    push(stack, size as usize, status);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_space_rounds_up_without_understating_the_package() {
        assert_eq!(
            required_space_text(Some(512)),
            "At least 512 KB of disk space is required."
        );
        assert_eq!(
            required_space_text(Some(1024)),
            "At least 1 MB of disk space is required."
        );
        assert_eq!(
            required_space_text(Some(1025)),
            "At least 2 MB of disk space is required."
        );
        assert_eq!(
            required_space_text(None),
            "Required disk space is unavailable."
        );
    }

    #[test]
    fn executable_is_a_basename_not_an_arbitrary_launch_path() {
        assert!(executable_name("another-app.exe"));
        for value in [
            "",
            ".",
            "..",
            "../app.exe",
            r"C:\app.exe",
            r"dir\app.exe",
            "a\".exe",
        ] {
            assert!(!executable_name(value));
        }
    }

    #[test]
    fn one_plugin_can_be_configured_for_independent_products() {
        unsafe {
            for (name, executable) in [("Example App", "example.exe"), ("Second App", "other.exe")]
            {
                let mut stack = null_mut();
                for value in [
                    "",
                    "Installed successfully.",
                    executable,
                    "A desktop app.",
                    name,
                ] {
                    push(&mut stack, 1024, value);
                }
                Configure(null_mut(), 1024, null_mut(), &mut stack, null_mut());
                assert_eq!(pop(&mut stack, 1024), "");
                assert!(stack.is_null());
                let branding = current();
                assert_eq!(branding.name, name);
                assert_eq!(branding.executable, executable);
                assert_eq!(branding.installed_message, "Installed successfully.");
            }
        }
    }
}
