use std::collections::{BTreeMap, BTreeSet};

use sc2bank::{BankDocument, Key, KeyId, Section, SectionId, ValueId, ValueKind, ValueNode};

use crate::{
	Control, Profile, ProfileError,
	definition::{FieldPath, Occurrence},
};

const MAX_PROFILES: usize = 64;

/// Canonical identity evidence supplied by the bank's local context.
#[derive(Clone, Copy, Debug, Default)]
pub struct BankIdentity<'a> {
	/// Exact author handle, if the file's identity has been established.
	pub author: Option<&'a str>,
	/// Bank filename without the extension.
	pub bank_name: &'a str,
	/// Map version from a trusted local source.
	pub map_version: Option<&'a str>,
	/// Map fingerprint from a trusted local source.
	pub fingerprint: Option<&'a str>,
}

/// The identity and shape evidence supporting a profile match.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
	/// Author, legacy bank name, and all declared field mappings match.
	Legacy,
	/// Exact author, bank name, and declared schema match.
	IdentityAndSchema,
	/// Exact identity and the configured map version match.
	Version,
	/// Exact identity and the configured map fingerprint match.
	Fingerprint,
	/// Exact identity, the configured map version and fingerprint match.
	VersionAndFingerprint,
}

/// The applicable profile, if one can be selected safely.
#[derive(Debug)]
#[non_exhaustive]
pub enum MatchOutcome<'a> {
	/// No known profile has the bank identity.
	Unknown,
	/// An identity matched but the required evidence, paths or values did not.
	Incompatible,
	/// Multiple profiles share the highest matching confidence.
	Ambiguous(Vec<&'a str>),
	/// One profile matched the bank and all of its declared fields.
	Matched(BoundProfile<'a>),
}

/// A validated set of profiles with unique identifiers.
#[derive(Debug, Default)]
pub struct ProfileRegistry {
	profiles: Vec<Profile>,
}

impl ProfileRegistry {
	/// Builds a registry with deterministic profile ordering.
	///
	/// # Errors
	///
	/// Rejects duplicate identifiers and registries larger than the profile limit.
	pub fn new(mut profiles: Vec<Profile>) -> Result<Self, ProfileError> {
		if profiles.len() > MAX_PROFILES {
			return Err(ProfileError::Invalid("registry size"));
		}
		profiles.sort_unstable_by(|left, right| left.id.cmp(&right.id));
		for pair in profiles.windows(2) {
			if pair[0].id == pair[1].id {
				return Err(ProfileError::DuplicateProfile(pair[0].id.clone()));
			}
		}
		Ok(Self { profiles })
	}

	/// Resolves the highest-confidence matching profile against the exact bank snapshot.
	#[must_use]
	pub fn match_bank<'a>(
		&'a self,
		bank: &'a BankDocument,
		identity: &BankIdentity<'_>,
	) -> MatchOutcome<'a> {
		let mut addresses = None;
		let mut best: Option<BoundProfile<'a>> = None;
		let mut ties = Vec::new();
		let mut known = false;
		for profile in &self.profiles {
			if !matches_identity(profile, identity) {
				continue;
			}
			known = true;
			let Some(confidence) = evidence(profile, identity) else {
				continue;
			};
			if best
				.as_ref()
				.is_some_and(|current| confidence < current.confidence)
			{
				continue;
			}
			let index = addresses.get_or_insert_with(|| AddressIndex::new(bank));
			let Some(candidate) = BoundProfile::bind(profile, confidence, index) else {
				continue;
			};
			if best
				.as_ref()
				.is_some_and(|current| confidence == current.confidence)
			{
				ties.push(profile.id());
			} else {
				best = Some(candidate);
				ties.clear();
			}
		}
		match best {
			Some(profile) if ties.is_empty() => MatchOutcome::Matched(profile),
			Some(profile) => {
				ties.insert(0, profile.id());
				MatchOutcome::Ambiguous(ties)
			}
			None if known => MatchOutcome::Incompatible,
			None => MatchOutcome::Unknown,
		}
	}
}

fn matches_identity(profile: &Profile, evidence: &BankIdentity<'_>) -> bool {
	evidence.author == Some(profile.identity.author.as_str())
		&& if profile.legacy {
			// The archived Go matcher lowercased each Unicode scalar independently.
			let lower = |scalar: char| scalar.to_lowercase().next().unwrap_or(scalar);
			profile
				.identity
				.bank
				.chars()
				.map(lower)
				.eq(evidence.bank_name.chars().map(lower))
		} else {
			profile.identity.bank == evidence.bank_name
		}
}

fn evidence(profile: &Profile, identity: &BankIdentity<'_>) -> Option<Confidence> {
	let version = profile.identity.map_version.as_deref();
	let fingerprint = profile.identity.fingerprint.as_deref();
	if version.is_some_and(|version| identity.map_version != Some(version))
		|| fingerprint.is_some_and(|fingerprint| {
			!identity
				.fingerprint
				.is_some_and(|actual| fingerprint.eq_ignore_ascii_case(actual))
		}) {
		return None;
	}
	Some(match (version, fingerprint) {
		(Some(_), Some(_)) => Confidence::VersionAndFingerprint,
		(_, Some(_)) => Confidence::Fingerprint,
		(Some(_), _) => Confidence::Version,
		_ if profile.legacy => Confidence::Legacy,
		_ => Confidence::IdentityAndSchema,
	})
}

/// A profile whose field references are bound to a single document snapshot.
#[derive(Debug)]
pub struct BoundProfile<'a> {
	pub(crate) profile: &'a Profile,
	pub(crate) fields: Vec<BoundField<'a>>,
	confidence: Confidence,
}

impl<'a> BoundProfile<'a> {
	fn bind(
		profile: &'a Profile,
		confidence: Confidence,
		addresses: &mut AddressIndex<'a>,
	) -> Option<Self> {
		for predicate in &profile.identity.predicates {
			let attribute = addresses.resolve(&predicate.path)?.attribute()?;
			if attribute.value() != predicate.equals {
				return None;
			}
		}
		let mut fields = Vec::with_capacity(profile.fields.len());
		let mut used = BTreeSet::new();
		for (index, field) in profile.fields.iter().enumerate() {
			let node = addresses.resolve(&field.path)?;
			let attribute = node.attribute()?;
			if !used.insert(node.id())
				|| !field.control.accepts(attribute.kind(), attribute.value())
			{
				return None;
			}
			fields.push(BoundField {
				id: &field.id,
				label: &field.label,
				description: field.description.as_deref(),
				control: &field.control,
				target: node.id(),
				kind: attribute.kind(),
				lexeme: attribute.value(),
				read_only: field.read_only || profile.derived.contains(&index),
			});
		}
		Some(Self {
			profile,
			fields,
			confidence,
		})
	}

	/// Gets the definition ID.
	#[must_use]
	pub fn id(&self) -> &'a str {
		&self.profile.id
	}

	/// Gets the level of matched identity evidence.
	#[must_use]
	pub fn confidence(&self) -> Confidence {
		self.confidence
	}

	/// Yields profile field groups in definition order.
	pub fn groups(&self) -> impl Iterator<Item = BoundGroup<'_, 'a>> + '_ {
		self.profile.groups.iter().map(|group| BoundGroup {
			label: &group.label,
			description: group.description.as_deref(),
			fields: &self.fields[group.fields.clone()],
		})
	}
}

/// One display group of bound profile fields.
#[derive(Debug)]
pub struct BoundGroup<'binding, 'bank> {
	/// Friendly group name.
	pub label: &'bank str,
	/// Optional game-specific description.
	pub description: Option<&'bank str>,
	/// Bound controls, in display order.
	pub fields: &'binding [BoundField<'bank>],
}

/// A friendly control bound to one exact value occurrence and lexeme.
#[derive(Debug)]
pub struct BoundField<'a> {
	/// Field identifier within the profile.
	pub id: &'a str,
	/// Friendly field name.
	pub label: &'a str,
	/// Optional game-specific description.
	pub description: Option<&'a str>,
	/// Control kind and its exact mapping.
	pub control: &'a Control,
	/// Document-local target occurrence.
	pub target: ValueId,
	/// Storage kind observed in the bank.
	pub kind: &'a ValueKind,
	/// Current XML-decoded lexeme.
	pub lexeme: &'a str,
	/// Whether this field can only be changed by a rule or another view.
	pub read_only: bool,
}

impl BoundField<'_> {
	/// Gets the selected state for a checkbox using its declared lexemes.
	#[must_use]
	pub fn checked(&self) -> Option<bool> {
		match self.control {
			Control::Checkbox { checked, .. } => Some(self.lexeme == checked),
			_ => None,
		}
	}
}

type Named<'a, T> = BTreeMap<&'a str, Vec<&'a T>>;

struct AddressIndex<'a> {
	sections: Named<'a, Section>,
	keys: BTreeMap<SectionId, Named<'a, Key>>,
	children: BTreeMap<KeyId, Named<'a, ValueNode>>,
}

impl<'a> AddressIndex<'a> {
	fn new(bank: &'a BankDocument) -> Self {
		Self {
			sections: named(bank.sections(), Section::name),
			keys: BTreeMap::new(),
			children: BTreeMap::new(),
		}
	}

	fn resolve(&mut self, path: &FieldPath) -> Option<&'a ValueNode> {
		let section = select(&self.sections, &path.section)?;
		let keys = self
			.keys
			.entry(section.id())
			.or_insert_with(|| named(section.keys(), Key::name));
		let key = select(keys, &path.key)?;
		if let Some(child) = &path.child {
			let children = self.children.entry(key.id()).or_insert_with(|| {
				named(key.values(), |node| {
					node.namespace().is_none().then_some(node.name())
				})
			});
			select(children, child)
		} else if let [only] = key.values() {
			Some(only)
		} else {
			None
		}
	}
}

fn named<'a, T>(items: &'a [T], name: impl Fn(&'a T) -> Option<&'a str>) -> Named<'a, T> {
	let mut index = Named::new();
	for item in items {
		if let Some(name) = name(item) {
			index.entry(name).or_default().push(item);
		}
	}
	index
}

fn select<'a, T>(index: &Named<'a, T>, selector: &Occurrence) -> Option<&'a T> {
	let items = index.get(selector.name.as_str())?;
	match (selector.occurrence, items.as_slice()) {
		(Some(index), items) => items.get(index).copied(),
		(None, [single]) => Some(*single),
		_ => None,
	}
}
