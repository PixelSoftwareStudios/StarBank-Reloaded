const SC2_GAME_ID: &str = "S2";

/// Checks the lexical form shared by SC2 player and author handles.
///
/// The numeric fields are ASCII and may contain leading zeroes. The game ID is
/// case sensitive so callers can keep the exact input for signing.
#[must_use]
pub fn is_sc2_account_handle(value: &str) -> bool {
	let mut fields = value.split('-');
	let (Some(region), Some(game), Some(realm), Some(account), None) = (
		fields.next(),
		fields.next(),
		fields.next(),
		fields.next(),
		fields.next(),
	) else {
		return false;
	};
	game == SC2_GAME_ID
		&& [region, realm, account]
			.into_iter()
			.all(|field| !field.is_empty() && field.bytes().all(|byte| byte.is_ascii_digit()))
}
