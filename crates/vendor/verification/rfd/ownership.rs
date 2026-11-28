//! Namespaced test-only FFI for the actual patched GTK ownership functions.
use std::{cell::RefCell, ffi::c_void, path::PathBuf, ptr};

const LABEL_TYPE: ::glib_sys::GType = 1;
const OTHER_TYPE: ::glib_sys::GType = 2;
const FREED_BYTE: u8 = 0xff;

struct Buffer {
	pointer: *mut c_void,
	length: usize,
	frees: usize,
}

struct Widget {
	class: *mut ::gobject_sys::GTypeClass,
	instance: *mut ::gobject_sys::GTypeInstance,
}

#[derive(Default)]
struct State {
	single: *mut std::ffi::c_char,
	slist: *mut ::glib_sys::GSList,
	children: *mut ::glib_sys::GList,
	buffers: Vec<Buffer>,
	widgets: Vec<Widget>,
	selected: Vec<usize>,
	single_calls: usize,
	multiple_calls: usize,
	children_calls: usize,
	slist_freed: bool,
	children_freed: bool,
	unrefs: usize,
	errors: Vec<String>,
}

thread_local! {
	static STATE: RefCell<State> = RefCell::new(State::default());
}

struct Case;

impl Case {
	fn new() -> Self {
		STATE.with(|state| assert!(state.borrow().buffers.is_empty()));
		Self
	}

	fn assert_released(&self) {
		STATE.with(|state| {
			let state = state.borrow();
			assert!(state.errors.is_empty(), "{:?}", state.errors);
			for buffer in &state.buffers {
				assert_eq!(buffer.frees, 1, "each returned filename must be freed once");
			}
			assert!(state.slist.is_null() || state.slist_freed);
			assert!(state.children.is_null() || state.children_freed);
			assert_eq!(state.unrefs, 0, "child widgets are borrowed");
		});
	}
}

impl Drop for Case {
	fn drop(&mut self) {
		let state = STATE.with(|state| state.take());
		// SAFETY: Each allocation belongs to this case. Shims only poison buffers;
		// links already freed by the real GLib function are never freed again.
		unsafe {
			for buffer in state.buffers {
				::glib_sys::g_free(buffer.pointer);
			}
			if !state.slist_freed {
				::glib_sys::g_slist_free(state.slist);
			}
			if !state.children_freed {
				::glib_sys::g_list_free(state.children);
			}
			for widget in state.widgets {
				// SAFETY: These pointers came from Box::into_raw in this case;
				// no child-list callback owns or releases either allocation.
				drop(Box::from_raw(widget.instance));
				drop(Box::from_raw(widget.class));
			}
		}
	}
}

fn filename(bytes: &[u8]) -> *mut std::ffi::c_char {
	assert!(!bytes.contains(&0));
	// SAFETY: GLib supplies bytes.len()+1 writable bytes, including a trailing NUL.
	let pointer = unsafe {
		let pointer = ::glib_sys::g_malloc(bytes.len() + 1).cast::<u8>();
		assert!(!pointer.is_null());
		ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, bytes.len());
		pointer.add(bytes.len()).write(0);
		pointer
	};
	STATE.with(|state| {
		state.borrow_mut().buffers.push(Buffer {
			pointer: pointer.cast(),
			length: bytes.len(),
			frees: 0,
		})
	});
	pointer.cast()
}

pub(crate) fn check_getters(
	mut single: impl FnMut() -> Option<PathBuf>,
	mut multiple: impl FnMut() -> Vec<PathBuf>,
) {
	// The valid case comes first: the old implementation fails on its missing
	// release before reaching any NULL pointer that old CStr handling cannot accept.
	for (bytes, expected) in [
		(
			Some(b"/fixture/First.SC2Bank".as_slice()),
			Some(PathBuf::from("/fixture/First.SC2Bank")),
		),
		(None, None),
		(Some(b"/fixture/invalid-\xff.SC2Bank".as_slice()), None),
	] {
		let case = Case::new();
		let pointer = bytes.map_or(ptr::null_mut(), filename);
		STATE.with(|state| state.borrow_mut().single = pointer);
		assert_eq!(single(), expected);
		STATE.with(|state| assert_eq!(state.borrow().single_calls, 1));
		case.assert_released();
	}
	{
		let case = Case::new();
		assert!(multiple().is_empty());
		STATE.with(|state| assert_eq!(state.borrow().multiple_calls, 1));
		case.assert_released();
	}
	// Test normal populated ownership before the defensive NULL-node case, so
	// restoring only the old multi-getter fails an assertion before CStr(NULL).
	for null_node in [false, true] {
		let case = Case::new();
		let mut values = vec![
			filename(b"/fixture/A.SC2Bank"),
			filename(b"/fixture/invalid-\xff"),
			filename(b"/fixture/B.SC2Bank"),
		];
		if null_node {
			values.insert(2, ptr::null_mut());
		}
		let mut head = ptr::null_mut();
		for value in values {
			// SAFETY: GLib allocates the links; data remains case-owned until cleanup.
			head = unsafe { ::glib_sys::g_slist_append(head, value.cast()) };
		}
		STATE.with(|state| state.borrow_mut().slist = head);
		assert_eq!(
			multiple(),
			[
				PathBuf::from("/fixture/A.SC2Bank"),
				PathBuf::from("/fixture/B.SC2Bank")
			]
		);
		STATE.with(|state| assert_eq!(state.borrow().multiple_calls, 1));
		case.assert_released();
	}
	println!("Actual GTK getters passed six ownership cases.");
}

pub(crate) fn check_children(mut select: impl FnMut(*mut ::gtk_sys::GtkDialog)) {
	let case = Case::new();
	let mut head = ptr::null_mut();
	let mut label = 0;
	for kind in [LABEL_TYPE, OTHER_TYPE] {
		let class = Box::into_raw(Box::new(::gobject_sys::GTypeClass { g_type: kind }));
		let pointer = Box::into_raw(Box::new(::gobject_sys::GTypeInstance { g_class: class }));
		if kind == LABEL_TYPE {
			label = pointer as usize;
		}
		// SAFETY: Raw Box allocations stay live until Case::drop releases its list.
		head = unsafe { ::glib_sys::g_list_append(head, pointer.cast()) };
		STATE.with(|state| {
			state.borrow_mut().widgets.push(Widget {
				class,
				instance: pointer,
			})
		});
	}
	STATE.with(|state| state.borrow_mut().children = head);
	select(ptr::null_mut());
	STATE.with(|state| {
		let state = state.borrow();
		assert_eq!(state.children_calls, 1);
		assert_eq!(state.selected, [label]);
	});
	case.assert_released();
	println!("Actual GTK message helper released its list and retained borrowed children.");
}

pub(crate) mod glib_sys {
	use super::*;
	pub use ::glib_sys::*;

	pub unsafe extern "C" fn g_free(pointer: gpointer) {
		if pointer.is_null() {
			return;
		}
		STATE.with(|state| {
			let mut state = state.borrow_mut();
			if let Some(buffer) = state
				.buffers
				.iter_mut()
				.find(|buffer| buffer.pointer == pointer)
			{
				buffer.frees += 1;
				// SAFETY: This case-owned allocation remains live until Case::drop.
				// Poisoning only payload bytes detects a copy made after release.
				unsafe { ptr::write_bytes(pointer, FREED_BYTE, buffer.length) };
			} else {
				state.errors.push("free of unknown buffer".into());
			}
		});
	}

	pub unsafe extern "C" fn g_slist_free_full(head: *mut GSList, free: GDestroyNotify) {
		let valid = STATE.with(|state| {
			let mut state = state.borrow_mut();
			let valid = head == state.slist
				&& !state.slist_freed
				&& free.is_some_and(|callback| {
					ptr::fn_addr_eq(callback, g_free as unsafe extern "C" fn(gpointer))
				});
			if valid {
				state.slist_freed = true;
			} else {
				state
					.errors
					.push("wrong list head, callback, or repeated release".into());
			}
			valid
		});
		if valid {
			// SAFETY: The original live GLib list and validated callback are passed
			// through. No RefCell borrow survives the callback into tracked g_free.
			unsafe { ::glib_sys::g_slist_free_full(head, free) };
		}
	}

	pub unsafe extern "C" fn g_list_free(head: *mut GList) {
		let valid = STATE.with(|state| {
			let mut state = state.borrow_mut();
			let valid = head == state.children && !state.children_freed;
			if valid {
				state.children_freed = true;
			} else {
				state
					.errors
					.push("wrong child-list head or repeated release".into());
			}
			valid
		});
		if valid {
			// SAFETY: Only the original case-owned GLib links are released.
			unsafe { ::glib_sys::g_list_free(head) };
		}
	}
}

pub(crate) mod gtk_sys {
	use super::*;
	pub use ::gtk_sys::*;

	pub unsafe extern "C" fn gtk_file_chooser_get_filename(
		_: *mut GtkFileChooser,
	) -> *mut std::ffi::c_char {
		STATE.with(|state| {
			let mut state = state.borrow_mut();
			state.single_calls += 1;
			state.single
		})
	}
	pub unsafe extern "C" fn gtk_file_chooser_get_filenames(
		_: *mut GtkFileChooser,
	) -> *mut ::glib_sys::GSList {
		STATE.with(|state| {
			let mut state = state.borrow_mut();
			state.multiple_calls += 1;
			state.slist
		})
	}
	pub unsafe extern "C" fn gtk_message_dialog_get_message_area(
		_: *mut GtkMessageDialog,
	) -> *mut GtkWidget {
		ptr::null_mut()
	}
	pub unsafe extern "C" fn gtk_container_get_children(
		_: *mut GtkContainer,
	) -> *mut ::glib_sys::GList {
		STATE.with(|state| {
			let mut state = state.borrow_mut();
			state.children_calls += 1;
			state.children
		})
	}
	pub unsafe extern "C" fn gtk_label_get_type() -> ::glib_sys::GType {
		LABEL_TYPE
	}
	pub unsafe extern "C" fn gtk_label_set_selectable(
		label: *mut GtkLabel,
		setting: ::glib_sys::gboolean,
	) {
		STATE.with(|state| {
			let mut state = state.borrow_mut();
			if setting == ::glib_sys::GTRUE
				&& state.widgets.iter().any(|widget| {
					ptr::eq(
						widget.instance,
						label.cast::<::gobject_sys::GTypeInstance>(),
					)
				}) {
				state.selected.push(label as usize);
			} else {
				state
					.errors
					.push("unknown child or wrong selectable value".into());
			}
		});
	}
}

pub(crate) mod gobject_sys {
	use super::*;
	pub use ::gobject_sys::*;
	pub unsafe extern "C" fn g_object_unref(_: *mut GObject) {
		STATE.with(|state| state.borrow_mut().unrefs += 1);
	}
}
