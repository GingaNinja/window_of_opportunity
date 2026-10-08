use std::cell::RefCell;

use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::{CreateCompatibleBitmap, DeleteObject, GetDC, HBITMAP, InvalidateRect},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::{
                CDDS_ITEMPOSTPAINT, CDDS_ITEMPREPAINT, CDDS_PREPAINT, CDRF_DODEFAULT,
                CDRF_NOTIFYITEMDRAW, CDRF_NOTIFYPOSTERASE, CDRF_NOTIFYPOSTPAINT, HIMAGELIST,
                ILC_COLOR32, ImageList_Add, ImageList_Create, ImageList_Destroy, LVCF_FMT,
                LVCF_MINWIDTH, LVCF_WIDTH, LVCFMT_LEFT, LVCOLUMNW, LVM_GETITEMRECT,
                LVM_INSERTCOLUMN, LVM_SETCOLUMNWIDTH, LVM_SETEXTENDEDLISTVIEWSTYLE,
                LVM_SETIMAGELIST, LVM_SETITEMCOUNT, LVN_ITEMCHANGED, LVN_ODCACHEHINT,
                LVN_ODSTATECHANGED, LVS_EX_FULLROWSELECT, LVS_NOCOLUMNHEADER, LVS_OWNERDATA,
                LVS_REPORT, LVS_SHOWSELALWAYS, LVSIL_SMALL, NM_CUSTOMDRAW, NMCUSTOMDRAW, NMHDR,
            },
            WindowsAndMessaging::*,
        },
    },
    core::*,
};

use super::{super::element::Element, app::AppState, paint, stack::Rect, widgets::Widget};

/// A virtual list-view — the control owns only the COUNT
/// (LVS_OWNERDATA); rows come from the snapshot as they scroll into
/// view. Single-column, headerless report view = our list look.
pub fn create_list(parent: HWND, count: usize) -> HWND {
    let style = LVS_REPORT
        | LVS_OWNERDATA
        | LVS_NOCOLUMNHEADER
        | LVS_SHOWSELALWAYS
        | WS_TABSTOP.0
        | WS_BORDER.0
        | WS_CHILD.0
        | WS_VISIBLE.0;
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("SysListView32"),
            w!(""),
            WINDOW_STYLE(style),
            0,
            0,
            10,
            10, // arrange() positions it before anything is visible
            Some(parent),
            None,
            Some(HINSTANCE(GetModuleHandleW(None).unwrap().0)),
            None,
        )
        .expect("CreateWindowExW list")
    };
    unsafe {
        // Full-row select: without it, hit-testing is confined to the
        // item's icon+label box (LVIR_SELECTBOUNDS) — and for a virtual
        // item with no text that box is a ~48px stub, so clicks past it
        // never select and the highlight is a sliver. With this style
        // the whole row is the hit target.
        SendMessageW(
            hwnd,
            LVM_SETEXTENDEDLISTVIEWSTYLE,
            Some(WPARAM(LVS_EX_FULLROWSELECT as usize)),
            Some(LPARAM(LVS_EX_FULLROWSELECT as isize)),
        );
        SendMessageW(hwnd, LVM_SETITEMCOUNT, Some(WPARAM(count)), None);
    }
    // A report-view item lives in COLUMN SPACE — with no columns the rows
    // have zero width: nothing to hit-test, so clicks never select and no
    // selection notifications fire. One full-width column is the list's
    // body; its width is synced to the control after arrange.
    let mut column = LVCOLUMNW {
        mask: LVCF_FMT | LVCF_WIDTH | LVCF_MINWIDTH,
        fmt: LVCFMT_LEFT,
        cx: 600,
        cxMin: 600,
        ..Default::default()
    };
    unsafe {
        SendMessageW(
            hwnd,
            LVM_INSERTCOLUMN,
            Some(WPARAM(0)),
            Some(LPARAM(&mut column as *mut LVCOLUMNW as isize)),
        );
    }

    hwnd
}

fn set_row_height(list: HWND, himl_slot: &mut Option<HIMAGELIST>, height: i32) {
    unsafe {
        let himl = ImageList_Create(1, height, ILC_COLOR32, 1, 1);
        let hbm = CreateCompatibleBitmap(GetDC(Some(list)), 1, height);
        ImageList_Add(himl, hbm, Some(HBITMAP::default()));
        _ = DeleteObject(hbm.into());

        let _ = SendMessageW(
            list,
            LVM_SETIMAGELIST,
            Some(WPARAM(LVSIL_SMALL as usize)),
            Some(LPARAM(himl.0 as isize)),
        );
        if let Some(old) = *himl_slot {
            let _ = ImageList_Destroy(Some(old));
        }
        *himl_slot = Some(himl);
    }
}

/// The snapshot rows for the list with this hwnd — the datasource the
/// painting work draws from.
// `Vec<Box<Element>>` is the tree's node currency (see `Widget::List::rows`)
#[allow(clippy::vec_box)]
fn find_list_rows(widget: &mut Widget, hwnd: HWND) -> Option<&Vec<Box<Element>>> {
    match widget {
        Widget::List { hwnd: h, rows, .. } if *h == hwnd => Some(rows),
        Widget::Container { children, .. } => children
            .iter_mut()
            .find_map(|child| find_list_rows(child, hwnd)),
        _ => None,
    }
}

/// Post-arrange list sync. The row height rides on a small image list
/// (the ObjectListView trick — rows are uniform in report view), and it
/// must only be applied once the list has its real size: LVM_SETIMAGELIST
/// on a list still at its 10x10 creation size computes the item grid from
/// that tiny view and the blank band above row 0 sticks forever. mount and
/// patch therefore leave `image_list` as None ("pending"); applying it
/// here — right after arrange() — lands it on a properly sized control.
pub fn sync_lists(widget: &mut Widget) {
    match widget {
        Widget::List {
            hwnd,
            row_height,
            image_list,
            ..
        } => {
            if image_list.is_none() {
                set_row_height(*hwnd, image_list, *row_height);
            }
            unsafe {
                let mut rect = RECT::default();
                let _ = GetClientRect(*hwnd, &mut rect);
                SendMessageW(
                    *hwnd,
                    LVM_SETCOLUMNWIDTH,
                    Some(WPARAM(0)),
                    Some(LPARAM((rect.right - rect.left) as isize)),
                );
            }
        }
        Widget::Container { children, .. } => children.iter_mut().for_each(sync_lists),
        _ => {}
    }
}

/// WM_NOTIFY from a child control. The custom-draw stage chain is the
/// seam where row painting plugs in — the plumbing is here, the pixels
/// are yours.
pub fn notify(app: &RefCell<AppState>, lparam: LPARAM) -> LRESULT {
    unsafe {
        let hdr = &*(lparam.0 as *const NMHDR);
        match hdr.code {
            NM_CUSTOMDRAW => {
                let draw = &*(lparam.0 as *const NMCUSTOMDRAW);
                match draw.dwDrawStage {
                    CDDS_PREPAINT => {
                        // LRESULT(CDRF_NOTIFYITEMDRAW as isize)
                        LRESULT((CDRF_NOTIFYPOSTPAINT | CDRF_NOTIFYITEMDRAW) as isize)
                    }
                    CDDS_ITEMPREPAINT => {
                        // TODO(painting): paint the row here from
                        // find_list_rows(app…, hdr.hwndFrom)[draw.dwItemSpec]
                        // — stack boxes via stack::natural/arrange, pixels
                        // via DrawTextW/FillRect. Return
                        // CDRF_SKIPDEFAULT once we own the row's painting.
                        LRESULT((CDRF_NOTIFYPOSTPAINT | CDRF_NOTIFYPOSTERASE) as isize)
                    }
                    CDDS_ITEMPOSTPAINT => {
                        // the row rect — the control's `rc` is NOT filled
                        // for list-view custom draw, query it — and the
                        // row's snapshot element to paint
                        let row = draw.dwItemSpec;
                        let mut rect = RECT::default();
                        SendMessageW(
                            hdr.hwndFrom,
                            LVM_GETITEMRECT,
                            Some(WPARAM(row)),
                            Some(LPARAM(&mut rect as *mut RECT as isize)),
                        );
                        let area = Rect {
                            x: rect.left + 6, // add some padding for the overlay selection
                            y: rect.top,
                            w: rect.right - rect.left - 12, // add some padding for the overlay selection
                            h: rect.bottom - rect.top,
                        };
                        if let Ok(mut app) = app.try_borrow_mut() {
                            if let Some(element) = app
                                .root_widget
                                .as_mut()
                                .and_then(|root| find_list_rows(root, hdr.hwndFrom))
                                .and_then(|rows| rows.get(row))
                            {
                                paint::paint_tree(draw.hdc, element, area);
                            }
                        } else {
                            // A render is in flight (it borrows the app), and
                            // renders resize this very control — the redraw
                            // that brought us here is their own SetWindowPos
                            // or LVM_SETCOLUMNWIDTH. Painting now would read
                            // a half-updated tree, so the row is skipped — but
                            // this synchronous pass VALIDATES the control, so
                            // no WM_PAINT would follow and the list would stay
                            // blank (virtual rows draw empty). Re-invalidate:
                            // the queued WM_PAINT lands after the render
                            // returns and paints the rows for real.
                            let _ = InvalidateRect(Some(hdr.hwndFrom), None, false);
                        }
                        LRESULT(CDRF_DODEFAULT as isize)
                    }
                    _ => LRESULT(CDRF_DODEFAULT as isize),
                }
            }
            LVN_ODCACHEHINT => {
                // TODO(painting/live rows): the visible range just changed
                // (NMLVCACHEHINT iFrom..iTo) — the prefetch window the
                // macOS side covers with its dequeue pool.
                LRESULT(0)
            }
            LVN_ITEMCHANGED => {
                // TODO(on_select): selection changed — a future
                // Handler::Select rides here.
                LRESULT(0)
            }
            LVN_ODSTATECHANGED => LRESULT(0),
            _ => LRESULT(0),
        }
    }
}
