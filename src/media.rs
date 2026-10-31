use std::{
    ffi::{OsStr, OsString},
    fs, io,
    path::{Component, Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(unix)]
use std::{
    ffi::CString,
    os::fd::{AsRawFd, FromRawFd},
    os::unix::ffi::OsStrExt,
};

#[cfg(target_os = "linux")]
#[repr(C)]
struct OpenHow {
    flags: u64,
    mode: u64,
    resolve: u64,
}

use whatsrust::FileKind;

pub mod preview;

#[derive(Debug, PartialEq, Eq)]
pub enum MediaLaunchError {
    UnconfiguredMediaRoot,
    PlayerUnavailable,
    UnsafeExecutable,
    AbsolutePath,
    ParentTraversal,
    MalformedPath,
    MissingFile,
    NotFile,
    SymlinkEscape,
}

#[derive(Debug)]
pub struct LaunchPlan {
    executable: PathBuf,
    arguments: Vec<PathBuf>,
    #[cfg(unix)]
    _stable_file: fs::File,
}

impl PartialEq for LaunchPlan {
    fn eq(&self, other: &Self) -> bool {
        self.executable == other.executable && self.arguments == other.arguments
    }
}
impl Eq for LaunchPlan {}

impl LaunchPlan {
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn arguments(&self) -> &[PathBuf] {
        &self.arguments
    }
}

pub trait LaunchExecutor {
    fn spawn(&mut self, executable: &Path, arguments: &[PathBuf]) -> io::Result<()>;
}

pub struct CommandLaunchExecutor;

pub trait ChildReaper {
    fn reap(&mut self, child: Child);
}

struct AsyncChildReaper;

impl ChildReaper for AsyncChildReaper {
    fn reap(&mut self, child: Child) {
        drop(reap_child_async(child));
    }
}

impl CommandLaunchExecutor {
    pub fn spawn_with_reaper(
        executable: &Path,
        arguments: &[PathBuf],
        reaper: &mut impl ChildReaper,
    ) -> io::Result<()> {
        let spec = command_spec(executable, arguments);
        let mut command = Command::new(spec.executable());
        command
            .args(spec.arguments())
            .stdin(spec.stdin.to_stdio())
            .stdout(spec.stdout.to_stdio())
            .stderr(spec.stderr.to_stdio());
        #[cfg(unix)]
        if spec.separate_process_group {
            command.process_group(0);
        }
        command.spawn().map(|child| reaper.reap(child))
    }
}

impl LaunchExecutor for CommandLaunchExecutor {
    fn spawn(&mut self, executable: &Path, arguments: &[PathBuf]) -> io::Result<()> {
        Self::spawn_with_reaper(executable, arguments, &mut AsyncChildReaper)
    }
}

pub fn reap_child_async(mut child: Child) -> thread::JoinHandle<io::Result<ExitStatus>> {
    thread::spawn(move || child.wait())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardIo {
    Null,
}

impl StandardIo {
    fn to_stdio(self) -> Stdio {
        match self {
            Self::Null => Stdio::null(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct CommandSpec {
    executable: PathBuf,
    arguments: Vec<PathBuf>,
    pub stdin: StandardIo,
    pub stdout: StandardIo,
    pub stderr: StandardIo,
    #[cfg(unix)]
    pub separate_process_group: bool,
}

impl CommandSpec {
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn arguments(&self) -> &[PathBuf] {
        &self.arguments
    }
}

pub fn command_spec(executable: &Path, arguments: &[PathBuf]) -> CommandSpec {
    CommandSpec {
        executable: executable.into(),
        arguments: arguments.into(),
        stdin: StandardIo::Null,
        stdout: StandardIo::Null,
        stderr: StandardIo::Null,
        #[cfg(unix)]
        separate_process_group: true,
    }
}

pub fn execute_launch(
    plan: &LaunchPlan,
    executor: &mut (impl LaunchExecutor + ?Sized),
) -> io::Result<()> {
    executor.spawn(plan.executable(), plan.arguments())
}

pub fn resolve_media_player(value: Option<OsString>) -> Result<PathBuf, MediaLaunchError> {
    let executable = value
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("mpv"));
    validate_executable(&executable)?;
    Ok(executable)
}

pub fn media_player_from_environment() -> Result<PathBuf, MediaLaunchError> {
    resolve_media_player(std::env::var_os("WPTUI_MEDIA_PLAYER"))
}

pub fn resolve_media_opener(
    kind: &FileKind,
    image_viewer: Option<OsString>,
    media_player: Option<OsString>,
) -> Result<PathBuf, MediaLaunchError> {
    let executable = if matches!(kind, FileKind::Image) {
        image_viewer.unwrap_or_else(|| OsString::from("feh"))
    } else {
        media_player.unwrap_or_else(|| OsString::from("mpv"))
    };
    let executable = PathBuf::from(executable);
    validate_executable(&executable)?;
    Ok(executable)
}

pub fn media_opener_from_environment(kind: &FileKind) -> Result<PathBuf, MediaLaunchError> {
    resolve_media_opener(
        kind,
        std::env::var_os("WPTUI_IMAGE_VIEWER"),
        std::env::var_os("WPTUI_MEDIA_PLAYER"),
    )
}

pub fn resolve_document_opener(
    path: &Path,
    pdf_viewer: Option<OsString>,
    document_viewer: Option<OsString>,
) -> Result<PathBuf, MediaLaunchError> {
    let executable = if is_pdf(path) {
        pdf_viewer.unwrap_or_else(|| OsString::from("zathura"))
    } else {
        document_viewer.unwrap_or_else(|| OsString::from("libreoffice"))
    };
    let executable = PathBuf::from(executable);
    validate_executable(&executable)?;
    Ok(executable)
}

pub fn document_opener_from_environment(path: &Path) -> Result<PathBuf, MediaLaunchError> {
    resolve_document_opener(
        path,
        std::env::var_os("WPTUI_PDF_VIEWER"),
        std::env::var_os("WPTUI_DOCUMENT_VIEWER"),
    )
}

#[derive(Debug)]
pub struct MediaRoot {
    path: PathBuf,
    #[cfg(unix)]
    directory: fs::File,
}

impl PartialEq for MediaRoot {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}
impl Eq for MediaRoot {}

impl MediaRoot {
    pub fn new(path: &Path) -> Result<Self, MediaLaunchError> {
        let path = path
            .canonicalize()
            .map_err(|_| MediaLaunchError::UnconfiguredMediaRoot)?;
        #[cfg(unix)]
        let directory = {
            // Pin the root, refusing a symlink substituted after canonicalize.
            let name = CString::new(path.as_os_str().as_bytes())
                .map_err(|_| MediaLaunchError::UnconfiguredMediaRoot)?;
            let fd = unsafe {
                libc::open(
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if fd < 0 {
                return Err(MediaLaunchError::UnconfiguredMediaRoot);
            }
            unsafe { fs::File::from_raw_fd(fd) }
        };
        #[cfg(unix)]
        let is_dir = directory
            .metadata()
            .map_err(|_| MediaLaunchError::UnconfiguredMediaRoot)?
            .is_dir();
        #[cfg(not(unix))]
        let is_dir = path.is_dir();
        if !is_dir {
            return Err(MediaLaunchError::UnconfiguredMediaRoot);
        }
        Ok(Self {
            path,
            #[cfg(unix)]
            directory,
        })
    }

    pub fn plan_launch(
        &self,
        executable: &Path,
        relative_path: &Path,
    ) -> Result<LaunchPlan, MediaLaunchError> {
        validate_executable(executable)?;
        #[cfg(unix)]
        let stable_file = self.open_media_file(relative_path)?;
        #[cfg(not(unix))]
        let path = self.media_file(relative_path)?;
        #[cfg(unix)]
        let arguments = {
            use std::os::fd::AsRawFd;
            let fd = stable_file.as_raw_fd();
            // The child must inherit the descriptor so its FD path refers to
            // the already-open file, not a pathname replaced before spawn.
            if unsafe { libc::fcntl(fd, libc::F_SETFD, 0) } == -1 {
                return Err(MediaLaunchError::MissingFile);
            }
            let fd_path = if cfg!(target_os = "macos") {
                format!("/dev/fd/{fd}")
            } else {
                format!("/proc/self/fd/{fd}")
            };
            opener_arguments(executable, PathBuf::from(fd_path))
        };
        #[cfg(not(unix))]
        let arguments = opener_arguments(executable, path);
        Ok(LaunchPlan {
            executable: executable.into(),
            arguments,
            #[cfg(unix)]
            _stable_file: stable_file,
        })
    }

    /// Resolve a pathname for display only. Do not reopen this path for media
    /// reads: concurrent replacements can invalidate the containment check.
    pub fn media_file(&self, relative_path: &Path) -> Result<PathBuf, MediaLaunchError> {
        validate_media_path(relative_path)?;
        let path = self
            .path
            .join(relative_path)
            .canonicalize()
            .map_err(|_| MediaLaunchError::MissingFile)?;
        if !path.starts_with(&self.path) {
            return Err(MediaLaunchError::SymlinkEscape);
        }
        if !fs::metadata(&path)
            .map_err(|_| MediaLaunchError::MissingFile)?
            .is_file()
        {
            return Err(MediaLaunchError::NotFile);
        }
        Ok(path)
    }

    /// Open from the pinned root directory. No preview or player reads the
    /// pathname returned by `media_file` after its separate validation step.
    pub fn open_media_file(&self, relative_path: &Path) -> Result<fs::File, MediaLaunchError> {
        validate_media_path(relative_path)?;
        #[cfg(unix)]
        {
            self.open_media_file_unix(relative_path)
        }
        #[cfg(not(unix))]
        {
            fs::File::open(self.media_file(relative_path)?)
                .map_err(|_| MediaLaunchError::MissingFile)
        }
    }

    #[cfg(unix)]
    fn open_media_file_unix(&self, relative_path: &Path) -> Result<fs::File, MediaLaunchError> {
        // This pathname is only a candidate. Canonicalizing it preserves the
        // prior behavior for relative AND absolute in-root symlinks; no file
        // content is read until the kernel confines the actual open to the FD.
        let candidate = self
            .path
            .join(relative_path)
            .canonicalize()
            .map_err(|_| MediaLaunchError::MissingFile)?;
        let contained = candidate
            .strip_prefix(&self.path)
            .map_err(|_| MediaLaunchError::SymlinkEscape)?;
        if contained.as_os_str().is_empty() {
            return Err(MediaLaunchError::NotFile);
        }
        let name = CString::new(contained.as_os_str().as_bytes())
            .map_err(|_| MediaLaunchError::MalformedPath)?;

        #[cfg(target_os = "linux")]
        let descriptor = {
            // Linux 5.6+: one beneath-constrained resolution, not a sequence
            // of openat calls vulnerable to a renamed intermediate directory.
            let how = OpenHow {
                flags: (libc::O_RDONLY | libc::O_CLOEXEC) as u64,
                mode: 0,
                resolve: 0x08 | 0x02, // RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS
            };
            unsafe {
                libc::syscall(
                    libc::SYS_openat2,
                    self.directory.as_raw_fd(),
                    name.as_ptr(),
                    &how as *const OpenHow,
                    std::mem::size_of::<OpenHow>(),
                ) as libc::c_int
            }
        };
        #[cfg(target_os = "macos")]
        let descriptor = {
            // Old XNU versions may ignore unknown open flags. Require a
            // specific kernel rejection of an escape before trusting the flag.
            const BENEATH: libc::c_int = 0x1000; // Darwin O_RESOLVE_BENEATH
            let escaped = c"../";
            let probe = unsafe {
                libc::openat(
                    self.directory.as_raw_fd(),
                    escaped.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | BENEATH,
                )
            };
            if probe >= 0 {
                unsafe { libc::close(probe) };
                return Err(MediaLaunchError::MissingFile);
            }
            if io::Error::last_os_error().raw_os_error() != Some(libc::ENOTCAPABLE) {
                return Err(MediaLaunchError::MissingFile);
            }
            unsafe {
                libc::openat(
                    self.directory.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_CLOEXEC | BENEATH,
                )
            }
        };
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        return Err(MediaLaunchError::MissingFile);
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            if descriptor < 0 {
                return Err(
                    if io::Error::last_os_error().raw_os_error() == Some(libc::EXDEV) {
                        MediaLaunchError::SymlinkEscape
                    } else {
                        MediaLaunchError::MissingFile
                    },
                );
            }
            let file = unsafe { fs::File::from_raw_fd(descriptor) };
            if !file
                .metadata()
                .map_err(|_| MediaLaunchError::MissingFile)?
                .is_file()
            {
                return Err(MediaLaunchError::NotFile);
            }
            Ok(file)
        }
    }
}

fn validate_media_path(relative_path: &Path) -> Result<(), MediaLaunchError> {
    if relative_path.is_absolute() {
        return Err(MediaLaunchError::AbsolutePath);
    }
    if relative_path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(MediaLaunchError::ParentTraversal);
    }
    if relative_path.as_os_str().is_empty()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(MediaLaunchError::MalformedPath);
    }
    Ok(())
}

pub fn plan_media_launch(
    media_root: Option<&Path>,
    executable: Option<&Path>,
    relative_path: Option<&Path>,
) -> Result<Option<LaunchPlan>, MediaLaunchError> {
    let Some(relative_path) = relative_path else {
        return Ok(None);
    };
    let root = MediaRoot::new(media_root.ok_or(MediaLaunchError::UnconfiguredMediaRoot)?)?;
    let executable = executable.ok_or(MediaLaunchError::PlayerUnavailable)?;
    root.plan_launch(executable, relative_path).map(Some)
}

fn validate_executable(executable: &Path) -> Result<(), MediaLaunchError> {
    if is_mpv(executable) {
        return Ok(());
    }

    match executable.components().next() {
        Some(Component::Normal(name))
            if executable.components().count() == 1
                && name.to_string_lossy().chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
                }) =>
        {
            Ok(())
        }
        _ => Err(MediaLaunchError::UnsafeExecutable),
    }
}

fn opener_arguments(executable: &Path, media_path: PathBuf) -> Vec<PathBuf> {
    if is_mpv(executable) {
        vec![
            PathBuf::from("--force-window"),
            PathBuf::from("--keep-open=yes"),
            media_path,
        ]
    } else if is_feh(executable) {
        vec![PathBuf::from("--"), media_path]
    } else {
        vec![media_path]
    }
}

fn is_pdf(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}

fn is_mpv(executable: &Path) -> bool {
    executable.file_name() == Some(OsStr::new("mpv"))
}

fn is_feh(executable: &Path) -> bool {
    executable.file_name() == Some(OsStr::new("feh"))
}
