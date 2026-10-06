// Forwards d3d9.dll's exports, so renaming this DLL d3d9.dll lets the game load it on its own.

use std::ffi::c_void;
use std::sync::OnceLock;

use log::{error, info};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows::core::{PCSTR, PCWSTR};

const E_FAIL: i32 = -2147467259;

static REAL: OnceLock<usize> = OnceLock::new();

/// The real d3d9.dll, by absolute path: a copy renamed d3d9.dll would otherwise load itself.
fn real() -> usize {
    *REAL.get_or_init(|| unsafe {
        let mut buffer = [0u16; 260];
        let length = GetSystemDirectoryW(Some(&mut buffer)) as usize;
        if length == 0 || length >= buffer.len() {
            error!("Could not read the system directory, so d3d9.dll cannot be forwarded");
            return 0;
        }

        let mut path = buffer[..length].to_vec();
        path.extend("\\d3d9.dll\0".encode_utf16());

        match LoadLibraryW(PCWSTR(path.as_ptr())) {
            Ok(module) => {
                info!("Forwarding d3d9 calls to the system library");
                module.0 as usize
            }
            Err(err) => {
                error!("Could not load the system d3d9.dll: {err}. The game will not start.");
                0
            }
        }
    })
}

unsafe fn entry(name: &[u8]) -> Option<unsafe extern "system" fn() -> isize> {
    let module = real();
    if module == 0 {
        return None;
    }

    unsafe { GetProcAddress(HMODULE(module as *mut c_void), PCSTR(name.as_ptr())) }
}

/// Declares one export that calls straight through to the system library.
macro_rules! forward {
    ($name:ident ($($arg:ident: $ty:ty),*) -> $ret:ty, $fallback:expr) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "system" fn $name($($arg: $ty),*) -> $ret {
            match unsafe { entry(concat!(stringify!($name), "\0").as_bytes()) } {
                Some(found) => unsafe {
                    let call: unsafe extern "system" fn($($ty),*) -> $ret =
                        ::std::mem::transmute(found);
                    call($($arg),*)
                },
                None => $fallback,
            }
        }
    };
}

forward!(Direct3DCreate9(sdk_version: u32) -> *mut c_void, std::ptr::null_mut());
forward!(Direct3DCreate9Ex(sdk_version: u32, out: *mut *mut c_void) -> i32, E_FAIL);
forward!(Direct3DShaderValidatorCreate9() -> *mut c_void, std::ptr::null_mut());
forward!(D3DPERF_BeginEvent(colour: u32, name: *const u16) -> i32, 0);
forward!(D3DPERF_EndEvent() -> i32, 0);
forward!(D3DPERF_GetStatus() -> u32, 0);
forward!(D3DPERF_QueryRepeatFrame() -> i32, 0);
forward!(D3DPERF_SetMarker(colour: u32, name: *const u16) -> (), ());
forward!(D3DPERF_SetOptions(options: u32) -> (), ());
forward!(D3DPERF_SetRegion(colour: u32, name: *const u16) -> (), ());
