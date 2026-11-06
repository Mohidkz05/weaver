//! Finding audio devices and reading their names.

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, EDataFlow, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture,
    eConsole, eRender,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ};
use windows::core::{Error, Result};

/// The Windows object that hands out audio devices.
pub fn enumerator() -> Result<IMMDeviceEnumerator> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
}

/// The human-readable name Windows shows in Sound settings.
pub fn friendly_name(device: &IMMDevice) -> Result<String> {
    unsafe {
        let properties = device.OpenPropertyStore(STGM_READ)?;
        let name = properties.GetValue(&PKEY_Device_FriendlyName)?;
        let text = PropVariantToStringAlloc(&name)?;
        let result = text.to_string();
        // Windows allocated this string for us, so we must hand it back.
        CoTaskMemFree(Some(text.as_ptr() as *const _));
        Ok(result?)
    }
}

/// The speakers/headphones Windows is currently playing through.
pub fn default_output() -> Result<IMMDevice> {
    unsafe { enumerator()?.GetDefaultAudioEndpoint(eRender, eConsole) }
}

/// The first active output whose name contains `text`, if any.
pub fn find_output(text: &str) -> Result<Option<IMMDevice>> {
    find(eRender, |name| name.contains(text))
}

/// The first active device going in `direction` whose name passes `keep`.
fn find(direction: EDataFlow, keep: impl Fn(&str) -> bool) -> Result<Option<IMMDevice>> {
    unsafe {
        let devices = enumerator()?.EnumAudioEndpoints(direction, DEVICE_STATE_ACTIVE)?;
        for i in 0..devices.GetCount()? {
            let device = devices.Item(i)?;
            if keep(&friendly_name(&device)?) {
                return Ok(Some(device));
            }
        }
    }
    Ok(None)
}

/// The real microphone to mix in, plus its name.
///
/// With a `hint`, the first mic whose name contains it (ignoring case).
/// Otherwise Windows' default mic, unless that's VB-CABLE itself (which it
/// will be once Weaver makes the cable the default for Discord): then the
/// first real mic instead. Mixing the cable into itself would feed back.
pub fn pick_mic(hint: Option<&str>) -> Result<(IMMDevice, String)> {
    let is_cable = |name: &str| name.contains("CABLE");
    let not_found = |what: String| {
        Error::new(
            E_FAIL,
            format!("{what} Run `weaver devices` to see the mics Windows knows about."),
        )
    };

    let device = match hint {
        Some(hint) => {
            let hint = hint.to_lowercase();
            find(eCapture, |name| name.to_lowercase().contains(&hint))?
                .ok_or_else(|| not_found(format!("No mic matches \"{hint}\".")))?
        }
        None => {
            let default = unsafe { enumerator()?.GetDefaultAudioEndpoint(eCapture, eConsole) };
            match default {
                Ok(device) if !is_cable(&friendly_name(&device)?) => device,
                _ => find(eCapture, |name| !is_cable(name))?
                    .ok_or_else(|| not_found("No microphone found.".into()))?,
            }
        }
    };

    let name = friendly_name(&device)?;
    if is_cable(&name) {
        return Err(Error::new(
            E_FAIL,
            format!("{name} is Weaver's own output; pick a real mic with --mic."),
        ));
    }
    Ok((device, name))
}

pub fn print_all() -> Result<()> {
    let enumerator = enumerator()?;
    println!("Speakers and headphones:");
    print_devices(&enumerator, eRender)?;
    println!("Microphones:");
    print_devices(&enumerator, eCapture)?;
    Ok(())
}

fn print_devices(enumerator: &IMMDeviceEnumerator, direction: EDataFlow) -> Result<()> {
    unsafe {
        let devices = enumerator.EnumAudioEndpoints(direction, DEVICE_STATE_ACTIVE)?;
        for i in 0..devices.GetCount()? {
            println!("  {}", friendly_name(&devices.Item(i)?)?);
        }
    }
    Ok(())
}
