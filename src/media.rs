use std::{
    ffi::{OsStr, OsString},
    fs, io,
    path::{Component, Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use whatsrust::FileKind;

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

pub fn execute_launch(plan: &LaunchPlan, executor: &mut impl LaunchExecutor) -> io::Result<()> {
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

#[derive(Debug, PartialEq, Eq)]
pub struct MediaRoot(PathBuf);

impl MediaRoot {
    pub fn new(path: &Path) -> Result<Self, MediaLaunchError> {
        let path = path
            .canonicalize()
            .map_err(|_| MediaLaunchError::UnconfiguredMediaRoot)?;
        path.is_dir()
            .then_some(Self(path))
            .ok_or(MediaLaunchError::UnconfiguredMediaRoot)
    }

    pub fn plan_launch(
        &self,
        executable: &Path,
        relative_path: &Path,
    ) -> Result<LaunchPlan, MediaLaunchError> {
        validate_executable(executable)?;
        let path = self.media_file(relative_path)?;
        #[cfg(unix)]
        let stable_file = fs::File::open(&path).map_err(|_| MediaLaunchError::MissingFile)?;
        #[cfg(unix)]
        let arguments = {
            use std::os::fd::AsRawFd;
            let fd = stable_file.as_raw_fd();
            // The child must inherit this descriptor: /proc/self/fd resolves the already-open,
            // validated inode rather than a path that an attacker can replace before spawn.
            unsafe { libc::fcntl(fd, libc::F_SETFD, 0) };
            opener_arguments(executable, PathBuf::from(format!("/proc/self/fd/{fd}")))
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

    pub fn media_file(&self, relative_path: &Path) -> Result<PathBuf, MediaLaunchError> {
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

        let path = self
            .0
            .join(relative_path)
            .canonicalize()
            .map_err(|_| MediaLaunchError::MissingFile)?;
        if !path.starts_with(&self.0) {
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
