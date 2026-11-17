//! Read the list of files/folders the user copied in Windows Explorer (Ctrl+C),
//! so they can be pasted into a floppy.

/// What the OS clipboard currently holds, for deciding what Ctrl+V should paste.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ClipboardFiles {
    /// Changes every time anything is copied anywhere in the OS. Compare it
    /// with the value seen at an in-app copy to tell which copy is newer.
    pub seq: u32,
    /// Absolute paths of copied files and folders (empty if the clipboard
    /// holds no file list).
    pub paths: Vec<String>,
}

#[cfg(windows)]
pub fn clipboard_files() -> ClipboardFiles {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
        OpenClipboard,
    };
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

    const CF_HDROP: u32 = 15;
    let seq = unsafe { GetClipboardSequenceNumber() };
    let mut paths = Vec::new();
    unsafe {
        if IsClipboardFormatAvailable(CF_HDROP).is_ok() && OpenClipboard(HWND::default()).is_ok() {
            if let Ok(h) = GetClipboardData(CF_HDROP) {
                let hdrop = HDROP(h.0);
                let count = DragQueryFileW(hdrop, u32::MAX, None);
                for i in 0..count {
                    let len = DragQueryFileW(hdrop, i, None) as usize;
                    let mut buf = vec![0u16; len + 1];
                    let n = DragQueryFileW(hdrop, i, Some(&mut buf)) as usize;
                    paths.push(String::from_utf16_lossy(&buf[..n]));
                }
            }
            let _ = CloseClipboard();
        }
    }
    ClipboardFiles { seq, paths }
}

#[cfg(not(windows))]
pub fn clipboard_files() -> ClipboardFiles {
    ClipboardFiles::default()
}
