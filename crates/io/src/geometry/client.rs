//! One immutable native request per disposable process. Drop requests child killing;
//! Tokio does not await termination when the request future is dropped.
use super::{
    Conformer, Ensemble, EnsembleConformer, Error, ForceField, GENERATION_SEED_MAX,
    MAX_CONFORMER_STARTS, Optimized, Pin, Point3, Prepared, Starts, wire,
};
#[cfg(not(target_arch = "wasm32"))]
use std::process::Stdio;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant as BatchInstant;
use std::{path::PathBuf, time::Duration};
#[cfg(not(target_arch = "wasm32"))]
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
    time::Instant,
};
#[cfg(target_arch = "wasm32")]
use web_time::Instant as BatchInstant;

#[cfg(not(target_arch = "wasm32"))]
const MAX_STDERR_BYTES: usize = 64 * 1024;
pub(super) const MAX_HEAP_BYTES: usize = 512 * 1024 * 1024;
pub(super) const MAX_TIMEOUT: Duration = Duration::from_secs(120);
/// Keeps the worker console hidden. Copies: reshiki::workers, engine/reference.rs,
/// reshiki-chemistry's inchi/generator.rs, reshiki-windows' metafile_worker.rs.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub timeout: Duration,
    pub heap_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        let budget = reshiki_process_heap::policy::Capabilities::detect().resolve();
        Self {
            timeout: budget.geometry_timeout(),
            heap_bytes: budget.heap_bytes(),
        }
    }
}
impl Limits {
    pub fn capacity(self) -> reshiki_geometry::Capacity {
        reshiki_geometry::Capacity::for_heap(self.heap_bytes)
    }
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
    // The browser solves in-process and never launches it.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
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
        self.generate_seeded(prepared, field, super::DEFAULT_SEED)
            .await
    }
    /// Sample conformers with an explicit nonnegative random seed, using the
    /// requested force field. The same molecule and seed remain reproducible.
    /// Seeds must be at most [`super::GENERATION_SEED_MAX`].
    /// Clear retained drawing depth with [`Prepared::without_retained_depth`]
    /// for fresh ordinary embeddings. Relaxed metal coordination can still use
    /// the planar drawing as an approximate seed.
    pub async fn generate_seeded(
        &self,
        prepared: &Prepared,
        field: ForceField,
        seed: i32,
    ) -> Result<Optimized, Error> {
        self.execute(
            prepared.generation_request(field, seed)?,
            prepared.ids().len(),
            None,
            None,
        )
        .await
    }
    /// Return every valid conformer from one fresh, seeded embedding batch.
    pub async fn ensemble(
        &self,
        prepared: &Prepared,
        field: ForceField,
        seed: i32,
        count: u32,
        mode: Ensemble,
    ) -> Result<Optimized, Error> {
        self.execute(
            prepared.ensemble_request(field, seed, count, mode)?,
            prepared.ids().len(),
            None,
            None,
        )
        .await
    }

    /// Sample sequential seeded batches up to the caller's deadline. Any error
    /// after a successful batch returns the conformers already collected, with
    /// the reason in [`Starts::stopped`]. First-batch errors propagate.
    /// In a browser, the deadline is checked between in-process batches.
    pub async fn conformer_starts(
        &self,
        prepared: &Prepared,
        field: ForceField,
        starts: u32,
        base_seed: i32,
        mode: Ensemble,
        deadline: BatchInstant,
    ) -> Result<Starts, Error> {
        self.conformer_starts_with(prepared, field, starts, base_seed, mode, deadline, |_| {})
            .await
    }

    /// Like [`Self::conformer_starts`], with the cumulative count of valid
    /// conformers reported after each accepted batch.
    #[allow(clippy::too_many_arguments)]
    pub async fn conformer_starts_with(
        &self,
        prepared: &Prepared,
        field: ForceField,
        starts: u32,
        base_seed: i32,
        mode: Ensemble,
        deadline: BatchInstant,
        mut progress: impl FnMut(usize),
    ) -> Result<Starts, Error> {
        if !(1..=MAX_CONFORMER_STARTS).contains(&starts) {
            return Err(Error::Invalid(format!(
                "Conformer search needs 1–{MAX_CONFORMER_STARTS} starts"
            )));
        }
        let originals = prepared.ids().len();
        let batch_size = reshiki_geometry::MAX_CONFORMERS
            .min((reshiki_geometry::ENSEMBLE_ATOM_BUDGET / originals) as u32);
        // Validate the caller's base seed using a one-member template before
        // assigning the actual batch sizes and wrapped seeds below.
        let mut operation = prepared.ensemble_request(field, base_seed, 1, mode)?;
        let seed_range =
            i64::from(GENERATION_SEED_MAX) - i64::from(reshiki_geometry::MAX_CONFORMERS) + 1;
        let mut remaining = starts;
        let mut result = Starts {
            conformers: Vec::new(),
            batches: 0,
            stopped: None,
        };
        let mut hydrogen_parents = None;
        while remaining > 0 {
            if BatchInstant::now() >= deadline {
                result.stopped = Some("Conformer search deadline reached".into());
                break;
            }
            operation.conformers = batch_size.min(remaining);
            operation.seed = (i64::from(base_seed) + i64::from(result.batches) * 7919)
                .rem_euclid(seed_range) as i32;
            let batch = match self
                .execute(operation.clone(), originals, None, Some(deadline))
                .await
            {
                Ok(batch) => batch,
                Err(error) if result.batches > 0 => {
                    result.stopped = Some(
                        if matches!(error, Error::Timeout) && BatchInstant::now() >= deadline {
                            "Conformer search deadline reached".into()
                        } else {
                            error.to_string()
                        },
                    );
                    break;
                }
                Err(error) => return Err(error),
            };
            let parents = &batch.conformer.hydrogen_parents;
            if hydrogen_parents.get_or_insert_with(|| parents.clone()) != parents {
                result.stopped =
                    Some("Temporary hydrogen identities changed between batches".into());
                break;
            }
            result.conformers.extend(batch.ensemble);
            result.batches += 1;
            remaining -= operation.conformers;
            progress(result.conformers.len());
        }
        Ok(result)
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
            None,
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
            None,
        )
        .await
    }
    async fn execute(
        &self,
        operation: reshiki_geometry::Request,
        originals: usize,
        previous: Option<&Conformer>,
        overall_deadline: Option<BatchInstant>,
    ) -> Result<Optimized, Error> {
        self.limits.validate()?;
        let capacity = self.limits.capacity();
        operation
            .validate_capacity(capacity)
            .map_err(Error::Invalid)?;
        let field = operation.field;
        // Generation falls back to UFF when MMFF cannot type the molecule.
        let generating = operation.operation == reshiki_geometry::Operation::Generate;
        let iterations = operation.max_iterations;
        let evaluating = matches!(operation.operation, reshiki_geometry::Operation::Evaluate);
        let ensemble_limit = operation
            .ensemble
            .map_or(0, |_| operation.conformers as usize);
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
                capacity: Some(capacity),
                operation,
            },
            wire::MAX_REQUEST_BYTES,
        )
        .map_err(Error::Protocol)?;
        #[cfg(not(target_arch = "wasm32"))]
        let (_reservation, bytes) = {
            let executable = match &self.executable {
                Some(path) => path.clone(),
                None => std::env::current_exe()?,
            };
            // Waiting for a shared memory reservation consumes this operation's
            // deadline and remains cancellable with the async future.
            let budget = reshiki_process_heap::policy::Capabilities::detect().resolve();
            let reserved_bytes = self
                .limits
                .heap_bytes
                .checked_add(32 * 1024 * 1024)
                .ok_or_else(|| Error::Invalid("Geometry reservation size overflow".into()))?;
            if reserved_bytes > budget.reservation_ceiling() {
                return Err(Error::Invalid(
                    "Insufficient available memory for this geometry budget".into(),
                ));
            }
            let worker_deadline = Instant::now() + self.limits.timeout;
            let deadline = overall_deadline.map_or(worker_deadline, |overall| {
                worker_deadline.min(Instant::from_std(overall))
            });
            let reservation =
                reserve_until(deadline, || budget.try_reserve(reserved_bytes)).await?;
            (reservation, exchange(executable, &request, deadline).await?)
        };
        // The browser has no worker processes, so the request is solved in-process
        // through the same framing; the timeout and heap budget do not apply.
        #[cfg(target_arch = "wasm32")]
        let bytes = {
            if overall_deadline.is_some_and(|deadline| BatchInstant::now() >= deadline) {
                return Err(Error::Timeout);
            }
            super::worker::respond(&request).map_err(Error::Protocol)?
        };
        let response: wire::Response =
            wire::decode(&bytes, wire::MAX_RESPONSE_BYTES).map_err(Error::Protocol)?;
        if response.version != reshiki_geometry::RDKIT_VERSION {
            return Err(Error::Protocol(
                "Incompatible native geometry version".into(),
            ));
        }
        let response = response.result.map_err(Error::Invalid)?;
        if response.field != field
            && !(generating && response.field == reshiki_geometry::ForceField::UFF)
            || !response.energy.is_finite()
            || !response.initial_energy.is_finite()
            || response.diagnostics.len() > 100
            || response.diagnostics.iter().any(|s| s.len() > 8192)
            || response.approximations.len() > reshiki_geometry::MAX_ATOMS * 3 + 4
            || response.approximations.iter().any(|s| s.len() > 8192)
        {
            return Err(Error::Protocol(
                "Invalid force field, energy or diagnostics in response".into(),
            ));
        }
        let conformer = checked_conformer(
            response.coordinates,
            response.original_count,
            response.hydrogen_parents,
            originals,
            capacity.coordinates,
        )?;
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
        if response.ensemble.len() > ensemble_limit {
            return Err(Error::Protocol(
                "Response contains more ensemble members than requested".into(),
            ));
        }
        if ensemble_limit > 0 && response.ensemble.is_empty() {
            return Err(Error::Protocol(
                "Response omitted the requested ensemble".into(),
            ));
        }
        let mut ensemble = Vec::with_capacity(response.ensemble.len());
        for member in response.ensemble {
            if !member.energy.is_finite() {
                return Err(Error::Protocol("Invalid ensemble member energy".into()));
            }
            let conformer = checked_conformer(
                member.coordinates,
                conformer.original_atom_count,
                conformer.hydrogen_parents.clone(),
                originals,
                capacity.coordinates,
            )
            .map_err(|error| Error::Protocol(format!("Invalid ensemble member: {error}")))?;
            ensemble.push(EnsembleConformer {
                conformer,
                energy: member.energy,
                converged: member.converged,
            });
        }
        Ok(Optimized {
            conformer,
            ensemble,
            initial_energy: response.initial_energy,
            energy: response.energy,
            gradient,
            converged: response.converged,
            iterations,
            force_field: ForceField::from_native(response.field),
            diagnostics: response.diagnostics,
            approximations: response.approximations,
        })
    }
}

fn checked_conformer(
    coordinates: Vec<[f64; 3]>,
    original_atom_count: usize,
    hydrogen_parents: Vec<usize>,
    originals: usize,
    coordinate_capacity: usize,
) -> Result<Conformer, Error> {
    let conformer = Conformer {
        positions: coordinates
            .into_iter()
            .map(|[x, y, z]| Point3 { x, y, z })
            .collect(),
        original_atom_count,
        hydrogen_parents,
    };
    conformer.validate(originals)?;
    if conformer.positions.len() > coordinate_capacity {
        return Err(Error::Coordinates(
            "Conformer exceeds its machine-budget coordinate capacity",
        ));
    }
    Ok(conformer)
}

// Return ownership to execute so admission remains charged through response
// decoding and validation, just as it is while the child is running.
#[cfg(not(target_arch = "wasm32"))]
async fn reserve_until<R>(
    deadline: Instant,
    mut try_reserve: impl FnMut() -> Option<R>,
) -> Result<R, Error> {
    loop {
        if Instant::now() >= deadline {
            return Err(Error::Timeout);
        }
        if let Some(reservation) = try_reserve() {
            // Acquisition can be descheduled after its first deadline check.
            if Instant::now() >= deadline {
                drop(reservation);
                return Err(Error::Timeout);
            }
            return Ok(reservation);
        }
        tokio::time::sleep_until(deadline.min(Instant::now() + Duration::from_millis(10))).await;
    }
}

#[cfg(not(target_arch = "wasm32"))]
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
#[cfg(not(target_arch = "wasm32"))]
async fn exchange(
    executable: PathBuf,
    request: &[u8],
    deadline: Instant,
) -> Result<Vec<u8>, Error> {
    if Instant::now() >= deadline {
        return Err(Error::Timeout);
    }
    exchange_with_deadline(executable, request, tokio::time::sleep_until(deadline)).await
}

#[cfg(not(target_arch = "wasm32"))]
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
        command.creation_flags(CREATE_NO_WINDOW);
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
        // A deadline already ready on this poll must not poll child startup.
        biased;
        () = deadline => Err(Error::Timeout),
        result = operation => result,
    }
}

#[cfg(test)]
mod tests;
