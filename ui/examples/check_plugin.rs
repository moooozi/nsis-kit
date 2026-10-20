//! Verify DLL loading and NSIS exports in a matching-architecture process.
use std::{env, io, process};
use windows_sys::Win32::{
    Foundation::FreeLibrary,
    System::LibraryLoader::{GetProcAddress, LoadLibraryW},
};

fn main() {
    let path = env::args()
        .nth(1)
        .expect("pass the absolute plug-in DLL path");
    let path: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let module = LoadLibraryW(path.as_ptr());
        if module.is_null() {
            eprintln!("Plug-in load failed: {}", io::Error::last_os_error());
            process::exit(1);
        }
        let fixture = GetProcAddress(module, c"FixtureOnly".as_ptr().cast()).is_some();
        if (fixture && env::args().any(|arg| arg == "--production"))
            || fixture != cfg!(feature = "fixture-engine")
        {
            eprintln!("Fixture/production plugin features do not match the requested build.");
            FreeLibrary(module);
            process::exit(1);
        }
        let mut names = vec![
            c"Configure",
            c"RequiredSpace",
            c"ConfigureMaintenance",
            c"Choose",
            c"Complete",
            c"Select",
        ];
        if cfg!(feature = "installer") {
            names.extend([
                c"InterfaceVersion",
                c"Run",
                c"Execute",
                c"Report",
                c"IsElevated",
                c"UninstallOptions",
                c"HandoffForeground",
                c"Tokenize",
                c"NormalizeWorkerArguments",
                c"ReadRegistryList",
                c"WriteRegistryList",
                c"PrepareShutdown",
                c"Launch",
            ]);
        }
        for name in names {
            if GetProcAddress(module, name.as_ptr().cast()).is_none() {
                eprintln!("Missing NSIS export: {name:?}");
                FreeLibrary(module);
                process::exit(1);
            }
        }
        FreeLibrary(module);
    }
    println!("Plug-in loads and all expected NSIS exports are available.");
}
