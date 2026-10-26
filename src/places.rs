//! Where a provider's program is: on the PATH, the one a person gets by
//! typing its name, or else in the places its installers and its app or
//! editor extension put it. Shared by Codex and Claude Code.

use std::io::Read;
use std::path::{Path, PathBuf};

/// The most of a shim read: npm's are a few hundred bytes.
const SHIM_MOST: u64 = 64 * 1024;

/// What a person gets by typing `name`: in each absolute folder on the
/// PATH, in order, `name.exe`, or else the program an npm shim `name.cmd`
/// starts, as `program` finds it from the file the shim names. The shim is
/// read as text, never run, so no shell is needed; the current folder and
/// relative PATH entries are not searched, so nothing is run by chance. A
/// shim whose program is not there counts as no shim.
pub fn typed(
    path: Option<std::ffi::OsString>,
    name: &str,
    program: impl Fn(&Path) -> Option<PathBuf>,
) -> Option<PathBuf> {
    std::env::split_paths(&path?)
        .filter(|dir| dir.is_absolute())
        .find_map(|dir| {
            let exe = dir.join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
            shim_target(&dir.join(format!("{name}.cmd"))).and_then(|target| program(&target))
        })
}

/// The file an npm shim starts, when it is there.
fn shim_target(shim: &Path) -> Option<PathBuf> {
    let mut bytes = Vec::new();
    std::fs::File::open(shim)
        .ok()?
        .take(SHIM_MOST)
        .read_to_end(&mut bytes)
        .ok()?;
    let target = shim
        .parent()?
        .join(target_in(&String::from_utf8_lossy(&bytes))?);
    target.is_file().then_some(target)
}

/// In an npm shim's text, the file it starts: the last quoted path from the
/// shim's own folder (`"%dp0%\…"`, or `"%~dp0\…"`) that is not Node itself,
/// as that folder's relative path. One that climbs out with `..` is not
/// followed.
fn target_in(text: &str) -> Option<String> {
    text.lines()
        .flat_map(|line| line.split('"').skip(1).step_by(2))
        .filter_map(|quoted| {
            let lower = quoted.to_ascii_lowercase();
            ["%dp0%\\", "%~dp0\\"]
                .iter()
                .find(|from| lower.starts_with(*from))
                .map(|from| quoted[from.len()..].to_string())
        })
        .filter(|rest| !rest.to_ascii_lowercase().ends_with("node.exe"))
        .filter(|rest| !rest.is_empty() && !rest.split(['\\', '/']).any(|part| part == ".."))
        .last()
}

/// An npm package's program, kept in a package of its own for this
/// platform (`platform`, in the same scope as `package`): inside the
/// package's own node_modules, as npm installs it, or beside the package,
/// as installs that lift dependencies up do; for the package as its folder
/// is found and as its links lead.
pub fn npm_platform(package: &Path, platform: &str, inside: &str) -> Option<PathBuf> {
    let mut roots = vec![package.to_path_buf()];
    if let Some(real) = std::fs::canonicalize(package).ok().map(plain) {
        if real != package {
            roots.push(real);
        }
    }
    roots
        .iter()
        .flat_map(|root| {
            let scope = root.parent().and_then(Path::file_name);
            [
                scope.map(|scope| root.join("node_modules").join(scope).join(platform)),
                root.parent().map(|scope| scope.join(platform)),
            ]
        })
        .flatten()
        .map(|folder| folder.join(inside))
        .find(|file| file.is_file())
}

/// A path without the `\\?\` that canonicalize puts before a drive, so it
/// reads and starts like any other.
fn plain(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path,
    }
}

/// The first place, in order, that has the program: a path under a folder
/// the environment names, where a `*` stands for any one folder name, such
/// as an app's build or an extension's version. Where a `*` matches more
/// than one, the most recently changed file.
pub fn in_places(
    places: &[(&str, &str)],
    root: impl Fn(&str) -> Option<PathBuf>,
) -> Option<PathBuf> {
    places.iter().find_map(|(name, place)| {
        let mut found = vec![root(name).filter(|r| r.is_absolute())?];
        for part in place.split('\\') {
            found = found
                .into_iter()
                .flat_map(|dir| {
                    if part.contains('*') {
                        std::fs::read_dir(&dir)
                            .into_iter()
                            .flatten()
                            .flatten()
                            .filter(|e| like(part, &e.file_name().to_string_lossy()))
                            .map(|e| e.path())
                            .collect()
                    } else {
                        vec![dir.join(part)]
                    }
                })
                .collect();
        }
        found
            .into_iter()
            .filter(|p| p.is_file())
            .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok())
    })
}

/// Whether a folder name fits a part with one `*`, ignoring case as
/// Windows does.
pub fn like(part: &str, name: &str) -> bool {
    let (part, name) = (part.to_ascii_lowercase(), name.to_ascii_lowercase());
    match part.split_once('*') {
        Some((head, tail)) => {
            name.len() >= head.len() + tail.len() && name.starts_with(head) && name.ends_with(tail)
        }
        None => part == name,
    }
}

/// The folder an environment variable names.
pub fn environment(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// npm's shim for a package whose `bin` is a script, as cmd-shim writes
    /// it.
    const SCRIPT_SHIM: &str = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n  SET PATHEXT=%PATHEXT:;.JS;=;%\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\node_modules\\@openai\\codex\\bin\\codex.js\" %*\r\n";
    /// And for one whose `bin` is a program, which it starts itself.
    const PROGRAM_SHIM: &str = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\"%dp0%\\node_modules\\@anthropic-ai\\claude-code\\bin\\claude.exe\"   %*\r\n";

    fn folder(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("limitpeeker-places-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn put(file: &Path, bytes: &[u8]) -> PathBuf {
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
        file.to_path_buf()
    }

    #[test]
    fn an_npm_shim_names_the_file_it_starts() {
        // The last quoted path from the shim's folder that is not Node; one
        // that climbs out of the folder is not followed.
        assert_eq!(
            target_in(SCRIPT_SHIM).as_deref(),
            Some(r"node_modules\@openai\codex\bin\codex.js")
        );
        assert_eq!(
            target_in(PROGRAM_SHIM).as_deref(),
            Some(r"node_modules\@anthropic-ai\claude-code\bin\claude.exe")
        );
        assert_eq!(
            target_in("@\"%~dp0\\global\\node_modules\\x\\bin\\x.js\" %*").as_deref(),
            Some(r"global\node_modules\x\bin\x.js")
        );
        assert_eq!(target_in("\"%dp0%\\..\\elsewhere\\x.exe\" %*"), None);
        assert_eq!(target_in("\"C:\\Tools\\x.exe\" %*"), None);
        assert_eq!(target_in("IF EXIST \"%dp0%\\node.exe\" (echo)"), None);
        assert_eq!(target_in(""), None);
    }

    #[test]
    fn typing_a_name_runs_the_first_folder_s_exe_or_npm_shim() {
        // Folder by folder in the PATH's order; in one folder the .exe
        // before the .cmd; a shim whose program is not there is passed by.
        let dir = folder("typed");
        let npm = dir.join("npm");
        put(&npm.join("codex.cmd"), SCRIPT_SHIM.as_bytes());
        let script = put(&npm.join(r"node_modules\@openai\codex\bin\codex.js"), b"");
        let program = put(&dir.join("program.exe"), b"");
        let tools = put(&dir.join(r"tools\codex.exe"), b"")
            .parent()
            .unwrap()
            .to_path_buf();
        let path = |dirs: &[&PathBuf]| Some(std::env::join_paths(dirs).unwrap());
        let found = |_: &Path| Some(program.clone());
        let named = std::cell::RefCell::new(None);
        let seen = |target: &Path| {
            named.replace(Some(target.to_path_buf()));
            Some(program.clone())
        };
        assert_eq!(
            typed(path(&[&npm, &tools]), "codex", seen),
            Some(program.clone())
        );
        assert_eq!(named.take(), Some(script.clone()));
        assert_eq!(
            typed(path(&[&tools, &npm]), "codex", found),
            Some(tools.join("codex.exe"))
        );
        put(&npm.join("codex.exe"), b"");
        assert_eq!(
            typed(path(&[&npm, &tools]), "codex", found),
            Some(npm.join("codex.exe"))
        );
        std::fs::remove_file(npm.join("codex.exe")).unwrap();
        assert_eq!(
            typed(path(&[&npm, &tools]), "codex", |_| None),
            Some(tools.join("codex.exe"))
        );
        std::fs::remove_file(&script).unwrap();
        assert_eq!(typed(path(&[&npm]), "codex", found), None);
        assert_eq!(typed(Some("npm".into()), "codex", found), None);
        assert_eq!(typed(None, "codex", found), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_npm_package_s_program_is_in_its_platform_package_inside_or_beside_it() {
        // npm puts the platform package inside the package's own
        // node_modules; installs that lift dependencies put it beside.
        let dir = folder("platform");
        let package = dir.join(r"node_modules\@openai\codex");
        std::fs::create_dir_all(&package).unwrap();
        let inside = r"vendor\x86_64-pc-windows-msvc\bin\codex.exe";
        assert_eq!(npm_platform(&package, "codex-win32-x64", inside), None);
        let beside = put(
            &dir.join(r"node_modules\@openai\codex-win32-x64")
                .join(inside),
            b"",
        );
        assert_eq!(
            npm_platform(&package, "codex-win32-x64", inside),
            Some(beside)
        );
        let nested = put(
            &package
                .join(r"node_modules\@openai\codex-win32-x64")
                .join(inside),
            b"",
        );
        assert_eq!(
            npm_platform(&package, "codex-win32-x64", inside),
            Some(nested)
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            plain(PathBuf::from(r"\\?\C:\x\y")),
            PathBuf::from(r"C:\x\y")
        );
        assert_eq!(
            plain(PathBuf::from(r"\\?\UNC\server\share")),
            PathBuf::from(r"\\?\UNC\server\share")
        );
    }
}
