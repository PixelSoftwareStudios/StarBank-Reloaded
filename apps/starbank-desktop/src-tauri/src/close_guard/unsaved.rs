use rfd::{AsyncMessageDialog, MessageButtons, MessageDialogResult, MessageLevel};
use tauri::WebviewWindow;

use super::CloseChoice;

const SAVE_LABEL: &str = "Save";
const DISCARD_LABEL: &str = "Discard";
const CANCEL_LABEL: &str = "Cancel";

pub(crate) async fn ask_unsaved(
	window: &WebviewWindow,
	title: &str,
	description: &str,
	offer_save: bool,
) -> CloseChoice {
	let buttons = if offer_save {
		MessageButtons::YesNoCancelCustom(
			SAVE_LABEL.into(),
			DISCARD_LABEL.into(),
			CANCEL_LABEL.into(),
		)
	} else {
		MessageButtons::OkCancel
	};
	let result = AsyncMessageDialog::new()
		.set_parent(window)
		.set_title(title)
		.set_description(description)
		.set_level(MessageLevel::Warning)
		.set_buttons(buttons)
		.show()
		.await;
	choice_from_response(&result, offer_save)
}

fn choice_from_response(result: &MessageDialogResult, offer_save: bool) -> CloseChoice {
	match result {
		MessageDialogResult::Custom(label) if offer_save && label == SAVE_LABEL => {
			CloseChoice::Save
		}
		MessageDialogResult::Custom(label) if offer_save && label == DISCARD_LABEL => {
			CloseChoice::Discard
		}
		// RFD 0.17.2's async GTK3 dialog returns response IDs for custom buttons.
		#[cfg(target_os = "linux")]
		MessageDialogResult::Yes if offer_save => CloseChoice::Save,
		#[cfg(target_os = "linux")]
		MessageDialogResult::No if offer_save => CloseChoice::Discard,
		MessageDialogResult::Ok if !offer_save => CloseChoice::Discard,
		_ => CloseChoice::Cancel,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn custom_responses_require_the_matching_three_button_prompt() {
		let cases = [
			(
				MessageDialogResult::Custom(SAVE_LABEL.into()),
				CloseChoice::Save,
			),
			(
				MessageDialogResult::Custom(DISCARD_LABEL.into()),
				CloseChoice::Discard,
			),
			(
				MessageDialogResult::Custom(CANCEL_LABEL.into()),
				CloseChoice::Cancel,
			),
			(
				MessageDialogResult::Custom("Unexpected".into()),
				CloseChoice::Cancel,
			),
			(MessageDialogResult::Cancel, CloseChoice::Cancel),
		];
		for (response, expected) in cases {
			assert!(
				choice_from_response(&response, true) == expected,
				"{response:?}"
			);
			assert!(
				choice_from_response(&response, false) == CloseChoice::Cancel,
				"{response:?}"
			);
		}
	}

	#[test]
	fn ok_only_discards_when_the_two_button_prompt_was_shown() {
		assert!(choice_from_response(&MessageDialogResult::Ok, false) == CloseChoice::Discard);
		assert!(choice_from_response(&MessageDialogResult::Ok, true) == CloseChoice::Cancel);
	}

	#[test]
	fn standard_yes_and_no_are_only_accepted_for_linux_three_button_prompts() {
		let cases = [
			(MessageDialogResult::Yes, CloseChoice::Save),
			(MessageDialogResult::No, CloseChoice::Discard),
		];
		for (response, gtk_choice) in cases {
			let expected = if cfg!(target_os = "linux") {
				gtk_choice
			} else {
				CloseChoice::Cancel
			};
			assert!(
				choice_from_response(&response, true) == expected,
				"{response:?}"
			);
			assert!(
				choice_from_response(&response, false) == CloseChoice::Cancel,
				"{response:?}"
			);
		}
	}
}
