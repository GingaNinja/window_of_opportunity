use windows::Win32::{Foundation::HINSTANCE, System::LibraryLoader::GetModuleHandleW};

pub struct WPModule {
    pub hinst: HINSTANCE,
    // cmdLine: &str,
}

impl WPModule {
    pub fn new() -> WPModule {
        unsafe {
            match GetModuleHandleW(None) {
                Ok(hinst) => WPModule {
                    hinst: hinst.into(),
                },
                Err(error) => {
                    println!("error getting hinstance: {:?}", error);
                    WPModule {
                        hinst: HINSTANCE::default(),
                    }
                }
            }
        }
    }

    pub fn get_hinstance(&self) -> HINSTANCE {
        self.hinst
    }
}
