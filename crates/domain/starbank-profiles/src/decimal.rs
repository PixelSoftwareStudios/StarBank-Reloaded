use std::cmp::Ordering;

pub(crate) const MAX_SCALE: u32 = 18;
const MAX_DECIMAL_BYTES: usize = 128;
const DECIMAL_RADIX: i128 = 10;

pub(crate) struct Decimal {
	units: i128,
	scale: u32,
}

impl Decimal {
	pub(crate) fn parse(value: &str, max_scale: u32) -> Option<Self> {
		if value.len() > MAX_DECIMAL_BYTES {
			return None;
		}
		let body = value.strip_prefix('-').unwrap_or(value);
		let (whole, fraction) = body
			.split_once('.')
			.map_or((body, None), |(whole, fraction)| (whole, Some(fraction)));
		if whole.is_empty() || !whole.bytes().all(|byte| byte.is_ascii_digit()) {
			return None;
		}
		match fraction {
			None => Some(Self {
				units: value.parse().ok()?,
				scale: 0,
			}),
			Some(fraction) => {
				let scale = u32::try_from(fraction.len()).ok()?;
				if scale == 0
					|| scale > max_scale
					|| !fraction.bytes().all(|byte| byte.is_ascii_digit())
				{
					return None;
				}
				let mut digits = value.get(..value.len() - fraction.len() - 1)?.to_owned();
				digits.push_str(fraction);
				Some(Self {
					units: digits.parse().ok()?,
					scale,
				})
			}
		}
	}

	pub(crate) fn scaled(self, scale: u32) -> Option<i128> {
		self.units
			.checked_mul(DECIMAL_RADIX.checked_pow(scale.checked_sub(self.scale)?)?)
	}

	fn compare(&self, other: &Self) -> Ordering {
		if self.units.is_negative() != other.units.is_negative() {
			return self.units.cmp(&other.units);
		}

		// Compare the decimal magnitudes without multiplying into an i128 overflow.
		let left = self.units.unsigned_abs().to_string();
		let right = other.units.unsigned_abs().to_string();
		let common_scale = self.scale.max(other.scale);
		// Zero cannot acquire a larger magnitude when its decimal places are aligned.
		let order = if self.units == 0 || other.units == 0 {
			self.units.unsigned_abs().cmp(&other.units.unsigned_abs())
		} else {
			let left_len =
				left.len() + usize::try_from(common_scale - self.scale).unwrap_or_default();
			let right_len =
				right.len() + usize::try_from(common_scale - other.scale).unwrap_or_default();
			left_len.cmp(&right_len).then_with(|| {
				left.bytes()
					.chain(std::iter::repeat(b'0'))
					.zip(right.bytes().chain(std::iter::repeat(b'0')))
					.take(left_len)
					.find_map(|(a, b)| (a != b).then(|| a.cmp(&b)))
					.unwrap_or(Ordering::Equal)
			})
		};
		if self.units.is_negative() {
			order.reverse()
		} else {
			order
		}
	}
}

pub(crate) fn in_range(value: &str, scale: u32, min: Option<&str>, max: Option<&str>) -> bool {
	let Some(actual) = Decimal::parse(value, scale) else {
		return false;
	};
	min.is_none_or(|min| Decimal::parse(min, scale).is_some_and(|min| actual.compare(&min).is_ge()))
		&& max.is_none_or(|max| {
			Decimal::parse(max, scale).is_some_and(|max| actual.compare(&max).is_le())
		})
}

pub(crate) fn render(units: i128, scale: usize) -> String {
	if scale == 0 {
		return units.to_string();
	}
	let magnitude = units.unsigned_abs();
	let digits = format!("{magnitude:0>width$}", width = scale + 1);
	let split = digits.len() - scale;
	let (integer, fraction) = digits.split_at(split);
	let sign = if units.is_negative() { "-" } else { "" };
	format!("{sign}{integer}.{fraction}")
}
