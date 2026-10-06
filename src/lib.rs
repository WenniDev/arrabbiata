mod configuration;
mod handlers;
mod helpers;
mod hook;
mod log;
mod proxy;
mod sys;
mod types;
mod upscore;

use crate::log::Logger;
use ::log::{error, info};
use configuration::Configuration;
use std::sync::LazyLock;
use url::Url;
use windows::Win32::Foundation::{HINSTANCE, TRUE};
use windows::Win32::System::Console::AllocConsole;
use windows::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};
use windows::core::BOOL;

pub static CONFIGURATION: LazyLock<Configuration> = LazyLock::new(|| match Configuration::load() {
    Ok(configuration) => configuration,
    Err(err) => {
        // An unreadable config must not take the game down; the defaults leave submission off.
        error!("{err:#}");
        error!("Falling back to the default configuration");
        Configuration::default()
    }
});

pub static TACHI_IMPORT_URL: LazyLock<String> = LazyLock::new(|| {
    Url::parse(&CONFIGURATION.tachi.base_url)
        .and_then(|base| base.join(&CONFIGURATION.tachi.import))
        .map(|url| url.to_string())
        .unwrap_or_else(|err| {
            error!("Could not build the Tachi import URL: {err:#}");
            String::new()
        })
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

            // No avs2-ea3.dll to hook at boot, so this relies on avs2-core.dll already being loaded.
            if let Err(err) = hook::init() {
                error!("{err:#}");
            }
        }
        DLL_PROCESS_DETACH => {
            if let Err(err) = hook::release() {
                error!("{err:#}");
            }
        }
        _ => {}
    }

    TRUE
}
