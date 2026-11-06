//! Finding audio devices and reading their names.

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, EDataFlow, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture,
    eConsole, eRender,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ};
use windows::core::Result;

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
    unsafe {
        let devices = enumerator()?.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        for i in 0..devices.GetCount()? {
            let device = devices.Item(i)?;
            if friendly_name(&device)?.contains(text) {
                return Ok(Some(device));
            }
        }
    }
    Ok(None)
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
