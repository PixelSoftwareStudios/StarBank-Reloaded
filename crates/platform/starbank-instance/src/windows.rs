#![forbid(unsafe_code)]

use std::{
	error::Error,
	ffi::OsString,
	fmt, io,
	os::windows::{
		ffi::OsStringExt,
		io::{AsHandle as _, OwnedHandle},
	},
	path::{Component, Path, PathBuf, Prefix},
	time::{Duration, Instant as AdmissionDeadline},
};

use tokio::{
	io::{AsyncReadExt as _, AsyncWriteExt as _},
	net::windows::named_pipe::{NamedPipeClient, NamedPipeServer},
	time::{Instant, sleep_until},
};
use windows_sys::Win32::Foundation::{
	ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_NO_DATA, ERROR_PIPE_BUSY,
};

use crate::{
	OpenRequest, QueueAdmission,
	frame::{ACK_LEN, HEADER_LEN, RequestHeader},
	windows_security as security,
};

#[path = "windows/deadline.rs"]
mod deadline;

use deadline::{before_deadline, deadline_error};

#[cfg(test)]
#[path = "windows/tests.rs"]
mod tests;

const MAX_NAMESPACE_LEN: usize = 64;
const STARTUP_BUDGET: Duration = Duration::from_secs(5);
const CONNECTION_BUDGET: Duration = Duration::from_secs(2);
const RETRY_INTERVAL: Duration = Duration::from_millis(40);
const BANK_EXTENSION: &str = "sc2bank";

/// Result of claiming an endpoint for the current logon session.
pub enum Claim {
	/// This process owns the endpoint and can start listening.
	Primary(Listener),
	/// The running instance acknowledged admission of the request.
	Forwarded(QueueAdmission),
}

/// The only server instance for this namespace and logon session.
pub struct Listener {
	server: NamedPipeServer,
	owner: security::LogonSid,
}

/// Owns the first pipe instance independently of the listener task.
///
/// Keep it while an editor session is active, including after the listener fails.
#[must_use = "retain the guard while the editor owns the endpoint"]
pub struct EndpointGuard {
	_handle: OwnedHandle,
}

/// Native operations that can fail without a launch request being delivered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PipeOperation {
	/// Read the current process's logon identity.
	Identify,
	/// Claim the unique server instance.
	Bind,
	/// Connect to the existing endpoint.
	Connect,
	/// Verify the connected server's logon identity.
	Authenticate,
	/// Accept a connection on the owned endpoint.
	Listen,
	/// Disconnect a finished connection before reuse.
	Disconnect,
}

/// Failure to claim the endpoint, send a request or receive its acknowledgement.
#[derive(Debug)]
#[non_exhaustive]
pub enum InstanceError {
	/// The namespace must be a short lower-case ASCII identifier.
	InvalidNamespace,
	/// A request contains a path outside the supported local `.SC2Bank` syntax.
	InvalidPath,
	/// A local operation failed before delivery, or while maintaining the listener.
	Pipe {
		operation: PipeOperation,
		source: io::Error,
	},
	/// Delivery may have happened. Retrying could deliver the request again.
	DeliveryUncertain { source: io::Error },
}

impl fmt::Display for InstanceError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::InvalidNamespace => formatter.write_str("invalid instance namespace"),
			Self::InvalidPath => {
				formatter.write_str("instance path is not a local SC2Bank candidate")
			}
			Self::Pipe { operation, source } => {
				write!(formatter, "instance pipe {operation:?} failed ({source})")
			}
			Self::DeliveryUncertain { source } => write!(
				formatter,
				"instance request delivery is uncertain ({source})"
			),
		}
	}
}

impl Error for InstanceError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::Pipe { source, .. } | Self::DeliveryUncertain { source } => Some(source),
			Self::InvalidNamespace | Self::InvalidPath => None,
		}
	}
}

/// Bind the local endpoint or forward one request to its existing owner.
///
/// Call from a Tokio runtime with I/O and time enabled. A namespace is a stable app-specific
/// identifier. It is scoped to the current logon SID at runtime. Requests carry absolute local
/// bank candidates, or an empty list for activation alone. The caller resolves startup paths.
///
/// # Errors
///
/// Returns an input or pipe error before delivery starts. Any failure once transmission starts
/// returns [`InstanceError::DeliveryUncertain`], including an invalid or lost acknowledgement.
pub async fn claim(namespace: &str, request: &OpenRequest) -> Result<Claim, InstanceError> {
	let namespace = validate_namespace(namespace)?;
	candidate_paths(request)?;
	let owner = security::LogonSid::current()
		.map_err(|source| pipe_error(PipeOperation::Identify, source))?;
	let address = owner.endpoint(namespace);
	let deadline = Instant::now() + STARTUP_BUDGET;
	loop {
		if Instant::now() >= deadline {
			return Err(pipe_error(PipeOperation::Connect, deadline_error()));
		}
		match security::bind(&address, &owner) {
			Ok(server) => return Ok(Claim::Primary(Listener { server, owner })),
			Err(error)
				if has_code(&error, ERROR_ACCESS_DENIED) || has_code(&error, ERROR_PIPE_BUSY) => {}
			Err(source) => return Err(pipe_error(PipeOperation::Bind, source)),
		}
		match security::connect(&address) {
			Ok(mut client) => {
				let _peer = owner
					.verify_server(&client)
					.map_err(|source| pipe_error(PipeOperation::Authenticate, source))?;
				let now = Instant::now();
				if now >= deadline {
					return Err(pipe_error(PipeOperation::Connect, deadline_error()));
				}
				return forward(
					&mut client,
					request,
					(now + CONNECTION_BUDGET).min(deadline),
				)
				.await
				.map(Claim::Forwarded);
			}
			Err(error)
				if has_code(&error, ERROR_PIPE_BUSY) || has_code(&error, ERROR_FILE_NOT_FOUND) =>
			{
				let now = Instant::now();
				sleep_until((now + RETRY_INTERVAL).min(deadline)).await;
			}
			Err(source) => return Err(pipe_error(PipeOperation::Connect, source)),
		}
	}
}

impl Listener {
	/// Duplicate this instance handle so a serving failure does not release ownership.
	///
	/// # Errors
	///
	/// Returns the operating-system error if the pipe handle cannot be duplicated.
	pub fn retain_endpoint(&self) -> io::Result<EndpointGuard> {
		let handle = self.server.as_handle().try_clone_to_owned()?;
		Ok(EndpointGuard { _handle: handle })
	}

	/// Admit path-only requests until the listener is dropped or a native listener error occurs.
	///
	/// Use nonblocking queue access, such as `try_lock`, inside the admission callback and report
	/// every candidate through [`QueueAdmission`]. Check the supplied deadline under the queue
	/// lock before mutating it. Synchronous admission cannot be preempted by the pipe deadline.
	/// The callback retains file-open authority. Dropping the future releases the listener.
	/// Tokio retains resources needed by in-flight pipe operations until completion.
	/// A callback error ends that connection before acknowledgement.
	///
	/// # Errors
	///
	/// Returns an error if a native listener operation fails. Individual malformed, unauthorized,
	/// or timed-out clients are disconnected and leave the listener available for later clients.
	pub async fn serve(
		mut self,
		mut admit: impl FnMut(Vec<PathBuf>, AdmissionDeadline) -> io::Result<QueueAdmission>,
	) -> Result<(), InstanceError> {
		loop {
			match self.server.connect().await {
				Ok(()) => {
					let deadline = Instant::now() + CONNECTION_BUDGET;
					let _ = before_deadline(
						deadline,
						receive(&mut self.server, &self.owner, &mut admit, deadline),
					)
					.await;
				}
				// A client can close the connection before the server starts accepting it.
				Err(error) if has_code(&error, ERROR_NO_DATA) => {}
				Err(source) => return Err(pipe_error(PipeOperation::Listen, source)),
			}
			self.server
				.disconnect()
				.map_err(|source| pipe_error(PipeOperation::Disconnect, source))?;
		}
	}
}

async fn forward(
	client: &mut NamedPipeClient,
	request: &OpenRequest,
	deadline: Instant,
) -> Result<QueueAdmission, InstanceError> {
	let frame = request.encode();
	let mut started = false;
	let outcome = before_deadline(deadline, async {
		started = true;
		client.write_all(&frame).await?;
		let mut response = [0; ACK_LEN];
		client.read_exact(&mut response).await?;
		QueueAdmission::decode(&response, request.paths().len()).map_err(io::Error::other)
	})
	.await;
	outcome.map_err(|source| {
		if started {
			InstanceError::DeliveryUncertain { source }
		} else {
			pipe_error(PipeOperation::Connect, source)
		}
	})
}

async fn receive(
	server: &mut NamedPipeServer,
	owner: &security::LogonSid,
	admit: &mut impl FnMut(Vec<PathBuf>, AdmissionDeadline) -> io::Result<QueueAdmission>,
	deadline: Instant,
) -> io::Result<()> {
	let _peer = owner.verify_client(server)?;
	let mut header = [0; HEADER_LEN];
	server.read_exact(&mut header).await?;
	let header = RequestHeader::decode(&header).map_err(io::Error::other)?;
	let mut body = vec![0; header.payload_len()];
	server.read_exact(&mut body).await?;
	let request = header.decode_body(&body).map_err(io::Error::other)?;
	let paths = candidate_paths(&request).map_err(io::Error::other)?;
	let count = paths.len();
	if Instant::now() >= deadline {
		return Err(deadline_error());
	}
	let admission = admit(paths, deadline.into_std())?;
	if Instant::now() >= deadline {
		return Err(deadline_error());
	}
	if admission.total() != count {
		return Err(io::Error::other(
			"instance admission did not account for the request",
		));
	}
	server.write_all(&admission.encode()).await?;
	// Tokio pipe flush does not wait for consumption. A normal client closes after reading the
	// acknowledgement. Disconnect only after closure, unexpected extra input, or the deadline.
	let mut extra = [0];
	let _ = server.read(&mut extra).await;
	Ok(())
}

fn candidate_paths(request: &OpenRequest) -> Result<Vec<PathBuf>, InstanceError> {
	request
		.paths()
		.iter()
		.map(|units| {
			let path = PathBuf::from(OsString::from_wide(units));
			if is_bank_candidate(&path) {
				Ok(path)
			} else {
				Err(InstanceError::InvalidPath)
			}
		})
		.collect()
}

/// Check whether a path is a syntactically valid local absolute launch candidate.
///
/// File authority, existence, and on-disk validity are checked by the caller when opening it.
#[must_use]
pub fn is_bank_candidate(path: &Path) -> bool {
	let mut components = path.components();
	matches!(components.next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
		&& path.is_absolute()
		&& !components.any(|component| match component {
			Component::ParentDir => true,
			Component::Normal(part) => part.as_encoded_bytes().contains(&b':'),
			_ => false,
		})
		&& path
			.extension()
			.is_some_and(|extension| extension.eq_ignore_ascii_case(BANK_EXTENSION))
}

pub(crate) struct Namespace<'a>(&'a str);

impl<'a> Namespace<'a> {
	pub(crate) fn as_str(&self) -> &'a str {
		self.0
	}
}

fn validate_namespace(namespace: &str) -> Result<Namespace<'_>, InstanceError> {
	if namespace.is_empty()
		|| namespace.len() > MAX_NAMESPACE_LEN
		|| !namespace.bytes().all(|byte| {
			byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
		}) {
		return Err(InstanceError::InvalidNamespace);
	}
	Ok(Namespace(namespace))
}

fn has_code(error: &io::Error, code: u32) -> bool {
	error
		.raw_os_error()
		.is_some_and(|native| native.cast_unsigned() == code)
}

fn pipe_error(operation: PipeOperation, source: io::Error) -> InstanceError {
	InstanceError::Pipe { operation, source }
}
