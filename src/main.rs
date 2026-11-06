use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, EDataFlow, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eRender,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, STGM_READ,
};
use windows::core::Result;

fn main() -> Result<()> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };

    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };

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
            let device = devices.Item(i)?;
            let properties = device.OpenPropertyStore(STGM_READ)?;
            let name = properties.GetValue(&PKEY_Device_FriendlyName)?;

            let text = PropVariantToStringAlloc(&name)?;
            println!("  {}", text.to_string()?);
            CoTaskMemFree(Some(text.as_ptr() as *const _));
        }
    }

    Ok(())
}
