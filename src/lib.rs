mod configuration;
mod dump;
mod log;
mod sys;

use crate::log::Logger;
use ::log::{error, info};
use configuration::Configuration;
use std::sync::LazyLock;
use windows::Win32::Foundation::{HINSTANCE, TRUE};
use windows::Win32::System::Console::AllocConsole;
use windows::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};
use windows::core::BOOL;

pub static CONFIGURATION: LazyLock<Configuration> = LazyLock::new(|| match Configuration::load() {
    Ok(configuration) => configuration,
    Err(err) => {
        // Upstream exits the process here. For a tool that only observes traffic, taking
        // the game down over an unreadable config file would be a worse outcome than
        // carrying on with defaults, so it is reported and the defaults are used.
        error!("{err:#}");
        error!("Falling back to the default configuration");
        Configuration::default()
    }
});

fn print_infos() {
    info!(
        "Starting Arrabbiata v{}-{}",
        env!("CARGO_PKG_VERSION"),
        option_env!("VERGEN_GIT_DESCRIBE").unwrap_or("unknown")
    );
    info!("DanceDanceRevolution GRAND PRIX hook, forked from mikado by adamaq01");

    if let Some(build_date) = option_env!("VERGEN_BUILD_DATE") {
        info!("Build date: {build_date}");
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case, unused_variables)]
extern "system" fn DllMain(
    dll_module: HINSTANCE,
    call_reason: u32,
    reserved: *mut core::ffi::c_void,
) -> BOOL {
    match call_reason {
        DLL_PROCESS_ATTACH => {
            let _ = unsafe { AllocConsole() };
            Logger::new().init();
            panic_log::initialize_hook(panic_log::Configuration::default());

            print_infos();

            if !CONFIGURATION.general.enable {
                info!("Disabled by configuration, no hooks installed");
                return TRUE;
            }

            // Unlike upstream, there is no avs2-ea3.dll on GRAND PRIX to hang a boot hook
            // on, so the property hook goes in directly. chainload.txt loads us after AVS
            // is up, which is what makes this safe.
            if let Err(err) = dump::init() {
                error!("{err:#}");
            }
        }
        DLL_PROCESS_DETACH => {
            if let Err(err) = dump::release() {
                error!("{err:#}");
            }
        }
        _ => {}
    }

    TRUE
}
