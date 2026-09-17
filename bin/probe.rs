//! TEMPORARY diagnostic probe — verifies the raw-pixel blit path
//! (NSBitmapImageRep → NSImage → NSImageView) that the ray-tracer uses,
//! by round-tripping known pixels through it. Safe to delete.

use std::cell::RefCell;

use cacao::{
    appkit::{App, AppDelegate, window::Window},
    foundation::{NSString, nil},
    image::ImageView,
    objc::{class, msg_send, runtime::Object, sel, sel_impl},
    view::View,
};

struct ProbeApp {
    window: Window,
    content: View,
    image: RefCell<Option<ImageView>>,
}

fn blit(view: &ImageView, w: usize, h: usize, pixels: &[u8]) {
    unsafe {
        let rep: *mut Object = msg_send![class!(NSBitmapImageRep), alloc];
        let space = NSString::new("NSDeviceRGBColorSpace");
        let (px_w, px_h): (isize, isize) = (w as isize, h as isize);
        let (bps, spp): (isize, isize) = (8, 4);
        let (row, bpp): (isize, isize) = ((w * 4) as isize, 32);
        let rep: *mut Object = msg_send![rep,
            initWithBitmapDataPlanes:nil
            pixelsWide:px_w pixelsHigh:px_h
            bitsPerSample:bps samplesPerPixel:spp
            hasAlpha:true isPlanar:false
            colorSpaceName:&*space
            bytesPerRow:row bitsPerPixel:bpp];
        let dst: *mut u8 = msg_send![rep, bitmapData];
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), dst, pixels.len());

        let image: *mut Object = msg_send![class!(NSImage), alloc];
        let image: *mut Object = msg_send![image, initWithSize:cacao::core_graphics::geometry::CGSize::new(w as f64, h as f64)];
        let _: () = msg_send![image, addRepresentation:rep];

        view.objc.with_mut(|obj| {
            let _: () = msg_send![obj, setImage:image];
        });
    }
}

impl AppDelegate for ProbeApp {
    fn did_finish_launching(&self) {
        let (w, h) = (8usize, 6usize);

        // a recognizable test pattern: red row 0, green row 1, blue row 2...
        let mut pixels = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            let rgb = match y % 3 {
                0 => [255, 0, 0],
                1 => [0, 255, 0],
                _ => [0, 0, 255],
            };
            for _ in 0..w {
                pixels.extend_from_slice(&rgb);
                pixels.push(255);
            }
        }

        let view = ImageView::new();
        view.objc.with_mut(|obj| unsafe {
            let _: () = msg_send![obj, setImageScaling: 2usize];
        });
        blit(&view, w, h, &pixels);
        *self.image.borrow_mut() = Some(view);

        self.window.set_content_view(&self.content);
        {
            let iv = self.image.borrow();
            let iv = iv.as_ref().unwrap();
            let view_obj = iv.objc.get(|obj| obj as *const Object as *mut Object);
            self.content.objc.with_mut(|obj| unsafe {
                let _: () = msg_send![obj, addSubview: view_obj];
            });
        }
        self.window.show();

        // --- verify the round trip ---
        unsafe {
            let view_obj: *mut Object = {
                let iv = self.image.borrow();
                let iv = iv.as_ref().unwrap();
                iv.objc.get(|obj| obj as *const Object as *mut Object)
            };

            let image: *mut Object = msg_send![view_obj, image];
            println!("image non-nil: {}", !image.is_null());

            let size: cacao::core_graphics::geometry::CGSize = msg_send![image, size];
            println!("image size: {:.0} x {:.0}", size.width, size.height);

            let reps: *mut Object = msg_send![image, representations];
            let count: usize = msg_send![reps, count];
            println!("representations: {count}");

            // read the pixels back out of the representation
            let rep: *mut Object = msg_send![reps, objectAtIndex: 0usize];
            let data: *mut u8 = msg_send![rep, bitmapData];
            let byte_at = |i: usize| *(data.add(i));
            println!(
                "pixel(0,0) rgba: {} {} {} {}   (expect 255 0 0 255)",
                byte_at(0),
                byte_at(1),
                byte_at(2),
                byte_at(3)
            );
            println!(
                "pixel(0,1) rgba: {} {} {} {}   (expect 0 255 0 255)",
                byte_at(w * 4),
                byte_at(w * 4 + 1),
                byte_at(w * 4 + 2),
                byte_at(w * 4 + 3)
            );
        }
    }

    fn should_terminate_after_last_window_closed(&self) -> bool {
        true
    }
}

fn main() {
    App::new(
        "com.probe.blit",
        ProbeApp {
            window: Window::default(),
            content: View::new(),
            image: RefCell::new(None),
        },
    )
    .run();
}
