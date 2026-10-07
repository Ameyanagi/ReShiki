//! One immutable native request per disposable process. Drop cancels the child.
use super::{Conformer, Error, ForceField, Optimized, Pin, Point3, Prepared, wire};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};

const MAX_STDERR_BYTES: usize = 64 * 1024;
pub(super) const MAX_HEAP_BYTES: usize = 512 * 1024 * 1024;
pub(super) const MAX_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub timeout: Duration,
    pub heap_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(60),
            heap_bytes: 256 * 1024 * 1024,
        }
    }
}
impl Limits {
    pub(super) fn validate(self) -> Result<(), Error> {
        if self.timeout.is_zero()
            || self.timeout > MAX_TIMEOUT
            || self.heap_bytes == 0
            || self.heap_bytes > MAX_HEAP_BYTES
        {
            return Err(Error::Invalid(
                "Worker limits require 0–120 seconds and 1–512 MiB".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct Client {
    executable: Option<PathBuf>,
    pub limits: Limits,
}
impl Client {
    /// A test or development executable must be explicit and absolute. Ordinary
    /// operation relaunches current_exe; it never searches PATH or runs a shell.
    pub fn new(executable: PathBuf, limits: Limits) -> Result<Self, Error> {
        limits.validate()?;
        if !executable.is_absolute() {
            return Err(Error::Invalid(
                "Geometry worker executable must be an absolute path".into(),
            ));
        }
        let metadata = executable.metadata()?;
        if !metadata.is_file() {
            return Err(Error::Invalid(
                "Geometry worker is not a regular executable".into(),
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 == 0 {
                return Err(Error::Invalid(
                    "Geometry worker file is not executable".into(),
                ));
            }
        }
        Ok(Self {
            executable: Some(executable.canonicalize()?),
            limits,
        })
    }
    pub async fn generate(
        &self,
        prepared: &Prepared,
        field: ForceField,
    ) -> Result<Optimized, Error> {
        self.execute(
            prepared.native_request(
                field,
                reshiki_geometry::Operation::Generate,
                None,
                &[],
                500,
            )?,
            prepared.ids().len(),
            None,
        )
        .await
    }
    pub async fn relax(
        &self,
        prepared: &Prepared,
        conformer: &Conformer,
        field: ForceField,
        pins: &[Pin],
        iterations: u32,
    ) -> Result<Optimized, Error> {
        self.execute(
            prepared.native_request(
                field,
                reshiki_geometry::Operation::Relax,
                Some(conformer),
                pins,
                iterations,
            )?,
            prepared.ids().len(),
            Some(conformer),
        )
        .await
    }
    /// Physical analytic gradient. Pins are applied to the supplied coordinates
    /// but are not sent as force-field constraints during evaluation.
    pub async fn evaluate(
        &self,
        prepared: &Prepared,
        conformer: &Conformer,
        field: ForceField,
        pins: &[Pin],
    ) -> Result<Optimized, Error> {
        self.execute(
            prepared.native_request(
                field,
                reshiki_geometry::Operation::Evaluate,
                Some(conformer),
                pins,
                1,
            )?,
            prepared.ids().len(),
            Some(conformer),
        )
        .await
    }
    async fn execute(
        &self,
        operation: reshiki_geometry::Request,
        originals: usize,
        previous: Option<&Conformer>,
    ) -> Result<Optimized, Error> {
        self.limits.validate()?;
        let field = operation.field;
        let iterations = operation.max_iterations;
        let evaluating = matches!(operation.operation, reshiki_geometry::Operation::Evaluate);
        let fixed = operation
            .fixed_atoms
            .iter()
            .map(|&i| {
                let point = operation
                    .coordinates
                    .get(i)
                    .copied()
                    .ok_or(Error::Coordinates("Missing pinned coordinates"))?;
                Ok((i, point))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let request = wire::encode(
            &wire::Request {
                heap_bytes: self.limits.heap_bytes,
                operation,
            },
            wire::MAX_REQUEST_BYTES,
        )
        .map_err(Error::Protocol)?;
        let executable = match &self.executable {
            Some(path) => path.clone(),
            None => std::env::current_exe()?,
        };
        let bytes = exchange(executable, &request, self.limits.timeout).await?;
        let response: wire::Response =
            wire::decode(&bytes, wire::MAX_RESPONSE_BYTES).map_err(Error::Protocol)?;
        if response.version != reshiki_geometry::RDKIT_VERSION {
            return Err(Error::Protocol(
                "Incompatible native geometry version".into(),
            ));
        }
        let response = response.result.map_err(Error::Invalid)?;
        if response.field != field
            || !response.energy.is_finite()
            || !response.initial_energy.is_finite()
            || response.diagnostics.len() > 100
            || response.diagnostics.iter().any(|s| s.len() > 8192)
        {
            return Err(Error::Protocol(
                "Invalid force field, energy or diagnostics in response".into(),
            ));
        }
        let conformer = Conformer {
            positions: response
                .coordinates
                .iter()
                .map(|&[x, y, z]| Point3 { x, y, z })
                .collect(),
            original_atom_count: response.original_count,
            hydrogen_parents: response.hydrogen_parents,
        };
        conformer.validate(originals)?;
        for (index, [x, y, z]) in fixed {
            let actual = conformer
                .positions
                .get(index)
                .ok_or_else(|| Error::Protocol("Pinned atom disappeared".into()))?;
            if (actual.x - x).abs() > 1e-9
                || (actual.y - y).abs() > 1e-9
                || (actual.z - z).abs() > 1e-9
            {
                return Err(Error::Protocol(
                    "Native relaxation moved a pinned atom".into(),
                ));
            }
        }
        if previous.is_some_and(|old| {
            old.hydrogen_parents != conformer.hydrogen_parents
                || old.positions.len() != conformer.positions.len()
        }) {
            return Err(Error::Protocol(
                "Temporary hydrogen identities changed".into(),
            ));
        }
        let gradient = response.gradient.map(|points| {
            points
                .into_iter()
                .map(|[x, y, z]| Point3 { x, y, z })
                .collect::<Vec<_>>()
        });
        if evaluating && gradient.is_none() {
            return Err(Error::Protocol(
                "Energy evaluation omitted the analytic gradient".into(),
            ));
        }
        if gradient.as_ref().is_some_and(|g| {
            g.len() != conformer.positions.len()
                || g.iter()
                    .any(|p| ![p.x, p.y, p.z].iter().all(|v| v.is_finite()))
        }) {
            return Err(Error::Protocol("Invalid analytic gradient".into()));
        }
        Ok(Optimized {
            conformer,
            initial_energy: response.initial_energy,
            energy: response.energy,
            gradient,
            converged: response.converged,
            iterations,
            force_field: ForceField::from_native(response.field),
            diagnostics: response.diagnostics,
        })
    }
}

async fn read_limited(
    mut stream: impl AsyncRead + Unpin,
    limit: usize,
    name: &'static str,
) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            return Ok(output);
        }
        if count > limit.saturating_sub(output.len()) {
            return Err(Error::Limit(name));
        }
        output.try_reserve(count).map_err(|_| Error::Limit(name))?;
        output.extend_from_slice(
            buffer
                .get(..count)
                .ok_or_else(|| Error::Protocol("Invalid stream length".into()))?,
        );
    }
}
async fn exchange(
    executable: PathBuf,
    request: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, Error> {
    exchange_with_deadline(executable, request, tokio::time::sleep(timeout)).await
}

async fn exchange_with_deadline(
    executable: PathBuf,
    request: &[u8],
    deadline: impl std::future::Future<Output = ()>,
) -> Result<Vec<u8>, Error> {
    let operation = async {
        let mut command = Command::new(executable);
        command
            .arg("--geometry-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        let mut child = command.spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Protocol("Missing geometry input pipe".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Protocol("Missing geometry output pipe".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Protocol("Missing geometry diagnostic pipe".into()))?;
        let writer = async {
            stdin.write_all(request).await?;
            stdin.shutdown().await?;
            drop(stdin);
            Ok::<_, Error>(())
        };
        let ((), response, diagnostic, status) = tokio::try_join!(
            writer,
            read_limited(stdout, wire::MAX_RESPONSE_BYTES, "response"),
            read_limited(stderr, MAX_STDERR_BYTES, "diagnostic"),
            async { child.wait().await.map_err(Error::Io) }
        )?;
        if !status.success() {
            return Err(Error::Exit {
                code: status.code(),
                diagnostic: String::from_utf8_lossy(&diagnostic).into_owned(),
            });
        }
        Ok(response)
    };
    tokio::select! {
        result = operation => result,
        () = deadline => Err(Error::Timeout),
    }
}

#[cfg(test)]
mod tests;
