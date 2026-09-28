use std::ffi::c_void;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use windows::Win32::Foundation::{
    CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_POINTER, E_UNEXPECTED, S_FALSE,
    S_OK,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS, HBITMAP,
};
use windows::Win32::System::Com::{
    IClassFactory, IClassFactory_Impl, IStream, STREAM_SEEK_CUR, STREAM_SEEK_END, STREAM_SEEK_SET,
};
use windows::Win32::UI::Shell::PropertiesSystem::{
    IInitializeWithStream, IInitializeWithStream_Impl,
};
use windows::Win32::UI::Shell::{
    IThumbnailProvider, IThumbnailProvider_Impl, WTS_ALPHATYPE, WTSAT_ARGB,
};
use windows_core::{BOOL, GUID, HRESULT, IUnknown, Interface, Ref, Result, implement};

const CLSID: GUID = GUID::from_u128(gaanim_thumbnail::WINDOWS_HANDLER_CLSID_U128);

/// Live objects and server locks; the DLL may unload when there are none.
static REFERENCES: AtomicUsize = AtomicUsize::new(0);

/// `Read` and `Seek` over the COM stream Explorer hands the handler, so the
/// archive reader only fetches the directory and the cover entry.
struct StreamReader(IStream);

impl Read for StreamReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let mut read = 0u32;
        let length = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
        // SAFETY: `buffer` is valid for `length` bytes and `read` for a u32.
        unsafe {
            self.0
                .Read(buffer.as_mut_ptr().cast(), length, Some(&mut read))
        }
        .ok()
        .map_err(std::io::Error::other)?;
        Ok(read as usize)
    }
}

impl Seek for StreamReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let (offset, origin) = match position {
            SeekFrom::Start(offset) => (offset as i64, STREAM_SEEK_SET),
            SeekFrom::End(offset) => (offset, STREAM_SEEK_END),
            SeekFrom::Current(offset) => (offset, STREAM_SEEK_CUR),
        };
        let mut new_position = 0u64;
        // SAFETY: `new_position` is valid for a u64.
        unsafe { self.0.Seek(offset, origin, Some(&mut new_position)) }
            .map_err(std::io::Error::other)?;
        Ok(new_position)
    }
}

#[implement(IThumbnailProvider, IInitializeWithStream)]
struct Provider {
    stream: Mutex<Option<IStream>>,
}

impl Provider {
    fn new() -> Self {
        REFERENCES.fetch_add(1, Ordering::SeqCst);
        Self {
            stream: Mutex::new(None),
        }
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        REFERENCES.fetch_sub(1, Ordering::SeqCst);
    }
}

impl IInitializeWithStream_Impl for Provider_Impl {
    fn Initialize(&self, stream: Ref<IStream>, _mode: u32) -> Result<()> {
        let stream = stream.ok()?.clone();
        let mut slot = self.stream.lock().map_err(|_| E_UNEXPECTED)?;
        if slot.is_some() {
            return Err(E_UNEXPECTED.into());
        }
        *slot = Some(stream);
        Ok(())
    }
}

impl IThumbnailProvider_Impl for Provider_Impl {
    fn GetThumbnail(&self, cx: u32, bitmap: *mut HBITMAP, alpha: *mut WTS_ALPHATYPE) -> Result<()> {
        if bitmap.is_null() || alpha.is_null() {
            return Err(E_POINTER.into());
        }
        let stream = self
            .stream
            .lock()
            .map_err(|_| E_UNEXPECTED)?
            .clone()
            .ok_or(E_UNEXPECTED)?;
        // No cover (a bundle from Gaanim 0.6.0) or a damaged file: Explorer
        // falls back to the file type icon.
        let png = gaanim_thumbnail::read(StreamReader(stream))
            .ok()
            .flatten()
            .ok_or(E_FAIL)?;
        let image = gaanim_thumbnail::scaled(&png, cx).map_err(|_| E_FAIL)?;
        let (width, height) = image.dimensions();
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                // Negative: rows run from the top, as in the image.
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: `info` describes a 32-bit top-down DIB and `bits` receives
        // its pixel memory, which holds `width * height` BGRA pixels.
        let handle = unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)? };
        if bits.is_null() {
            return Err(E_FAIL.into());
        }
        // SAFETY: see above; the section is exactly this large.
        let pixels =
            unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), image.as_raw().len()) };
        // RGBA to the BGRA layout of a DIB.
        for (target, [red, green, blue, alpha]) in pixels
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(image.as_raw().as_chunks::<4>().0)
        {
            *target = [*blue, *green, *red, *alpha];
        }
        // SAFETY: both pointers were checked above.
        unsafe {
            *bitmap = handle;
            *alpha = WTSAT_ARGB;
        }
        Ok(())
    }
}

#[implement(IClassFactory)]
struct Factory;

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<IUnknown>,
        iid: *const GUID,
        object: *mut *mut c_void,
    ) -> Result<()> {
        if object.is_null() {
            return Err(E_POINTER.into());
        }
        // SAFETY: checked above.
        unsafe { *object = std::ptr::null_mut() };
        if !outer.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let provider: IUnknown = Provider::new().into();
        // SAFETY: `iid` and `object` come from the caller of CreateInstance.
        unsafe { provider.query(iid, object) }.ok()
    }

    fn LockServer(&self, lock: BOOL) -> Result<()> {
        if lock.as_bool() {
            REFERENCES.fetch_add(1, Ordering::SeqCst);
        } else {
            REFERENCES.fetch_sub(1, Ordering::SeqCst);
        }
        Ok(())
    }
}

/// COM entry point: the class factory of the thumbnail handler.
///
/// # Safety
/// Called by COM with valid `clsid`, `iid` and `object` pointers.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(
    clsid: *const GUID,
    iid: *const GUID,
    object: *mut *mut c_void,
) -> HRESULT {
    if clsid.is_null() || iid.is_null() || object.is_null() {
        return E_POINTER;
    }
    // SAFETY: checked above.
    unsafe { *object = std::ptr::null_mut() };
    // SAFETY: checked above.
    if unsafe { *clsid } != CLSID {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory: IClassFactory = Factory.into();
    // SAFETY: `iid` and `object` come from COM.
    unsafe { factory.query(iid, object) }
}

/// COM entry point: whether the DLL can be unloaded.
#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    if REFERENCES.load(Ordering::SeqCst) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}
