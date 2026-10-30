//! Local PDF rasterization. A single page and bounded output keep previews small.
use windows::{
    Data::Pdf::{PdfDocument, PdfPageRenderOptions},
    Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream},
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};

pub struct Page {
    pub png: Vec<u8>,
    pub count: u32,
}

pub fn render(bytes: &[u8], number: u32) -> Result<Page, String> {
    // This function runs on a blocking worker. Balance each successful init on
    // that same thread, after all WinRT objects have been released.
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(|e| e.to_string())?;
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }
    let _apartment = Apartment;
    render_inner(bytes, number).map_err(|e| e.to_string())
}

fn render_inner(bytes: &[u8], number: u32) -> windows::core::Result<Page> {
    let input = InMemoryRandomAccessStream::new()?;
    let writer = DataWriter::CreateDataWriter(&input.GetOutputStreamAt(0)?)?;
    writer.WriteBytes(bytes)?;
    writer.StoreAsync()?.get()?;
    writer.DetachStream()?;
    input.Seek(0)?;
    let document = PdfDocument::LoadFromStreamAsync(&input)?.get()?;
    let count = document.PageCount()?;
    if number == 0 || number > count {
        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            0x80070057_u32 as i32,
        )));
    }
    let page = document.GetPage(number - 1)?;
    let size = page.Size()?;
    if !size.Width.is_finite()
        || !size.Height.is_finite()
        || size.Width <= 0.0
        || size.Height <= 0.0
    {
        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            0x80070057_u32 as i32,
        )));
    }
    // Fit within 1600 x 2200 pixels, including unusual page aspect ratios.
    let scale = (1600.0 / size.Width).min(2200.0 / size.Height);
    let options = PdfPageRenderOptions::new()?;
    options.SetDestinationWidth((size.Width * scale).round().max(1.0) as u32)?;
    options.SetDestinationHeight((size.Height * scale).round().max(1.0) as u32)?;
    let output = InMemoryRandomAccessStream::new()?;
    page.RenderWithOptionsToStreamAsync(&output, &options)?
        .get()?;
    page.Close()?;
    let reader = DataReader::CreateDataReader(&output.GetInputStreamAt(0)?)?;
    let length = u32::try_from(output.Size()?).map_err(|_| {
        windows::core::Error::from_hresult(windows::core::HRESULT(0x80070057_u32 as i32))
    })?;
    reader.LoadAsync(length)?.get()?;
    let mut png = vec![0; length as usize];
    reader.ReadBytes(&mut png)?;
    Ok(Page { png, count })
}
