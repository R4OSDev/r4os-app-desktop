//! Desktop admission of loader targets. Installed image targets are rooted
//! on the system drive; ordinary file paths retain the strict SDK contract.
use r4os_sdk::{
    abi as a,
    path::{self, FilePath},
    storage,
};

#[derive(Debug)]
pub enum Failure {
    Path(path::Error),
    Classification(storage::Error),
    WindowCapacity { console: bool },
    Arguments,
    Loader(storage::Error),
    Attachment,
}

impl From<Failure> for storage::Error {
    fn from(value: Failure) -> Self {
        match value {
            Failure::Classification(error) | Failure::Loader(error) => error,
            Failure::WindowCapacity { .. } => Self::Raw(a::program_handle_error_would_block),
            Failure::Arguments => Self::Length,
            Failure::Path(_) | Failure::Attachment => Self::InvalidProvider,
        }
    }
}

pub fn path(target: &[u8]) -> Result<FilePath, path::Error> {
    if target.first() != Some(&b'/') {
        return FilePath::parse(target);
    }
    // MODULES.JSON stores image targets, as does the existing R4STD subsystem
    // resolver. This is a loader-target boundary, not a new file-path syntax.
    if target.len() > path::FILE_PATH_MAX - 2 {
        return Err(path::Error::TooLong);
    }
    let mut dos = [0; path::FILE_PATH_MAX];
    dos[..2].copy_from_slice(b"C:");
    dos[2..target.len() + 2].copy_from_slice(target);
    FilePath::parse(&dos[..target.len() + 2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_subsystem_targets_use_the_original_strict_sdk_parser() {
        for (target, expected) in [
            (
                b"/R4OS/SUBSYSTEMS/r4os.basic/R4BASIC.R4X".as_slice(),
                b"C:\\R4OS\\SUBSYSTEMS\\r4os.basic\\R4BASIC.R4X".as_slice(),
            ),
            (
                b"/R4OS/SUBSYSTEMS/r4os.gb/R4GB.R4X",
                b"C:\\R4OS\\SUBSYSTEMS\\r4os.gb\\R4GB.R4X",
            ),
            (
                b"/R4OS/SUBSYSTEMS/r4os.snes/R4SNES.R4X",
                b"C:\\R4OS\\SUBSYSTEMS\\r4os.snes\\R4SNES.R4X",
            ),
        ] {
            assert!(matches!(
                FilePath::parse(target),
                Err(path::Error::WrongKind)
            ));
            assert_eq!(path(target).unwrap().as_bytes(), expected);
        }
        for target in [
            b"C:\\APPS\\TEST.R4X".as_slice(),
            b"D:/Apps/Test.R4X",
            b"TEST.R4X",
            "C:\\Grün\\TEST.R4X".as_bytes(),
        ] {
            assert_eq!(
                path(target).unwrap().as_bytes(),
                FilePath::parse(target).unwrap().as_bytes()
            );
        }
    }

    #[test]
    fn image_target_admission_keeps_invalid_paths_and_limits_rejected() {
        for target in [
            b"/../TEST.R4X".as_slice(),
            b"/R4OS/TEST?.R4X",
            b"/R4OS/TEST\0.R4X",
            b"/R4OS/\xff.R4X",
            b"\\R4OS\\TEST.R4X",
        ] {
            assert!(path(target).is_err(), "{target:?}");
        }
        let mut too_long = [b'a'; path::FILE_PATH_MAX - 1];
        too_long[0] = b'/';
        assert!(matches!(path(&too_long), Err(path::Error::TooLong)));
    }

    #[test]
    fn host_request_keeps_spaced_guest_path_and_opaque_launch_arguments() {
        let guest = b"D:\\Roms\\SNES\\Super Mario World (Europe).sfc";
        let mut encoded = [0; 128];
        let args = r4os_sdk::subsystems::encode_launch(guest, &[], &mut encoded).unwrap();
        let mut request = a::ProgramHostLaunchRequest::default();
        let target = b"/R4OS/SUBSYSTEMS/r4os.snes/R4SNES.R4X";
        request.path[..target.len()].copy_from_slice(target);
        request.args[..args.len()].copy_from_slice(args);
        request.policy = a::LaunchPolicy::gui.0;
        let launch = super::super::launch_request::request(&request, 0).unwrap();
        assert_eq!(launch.path(), target);
        assert_eq!(launch.args(), args);
        assert_eq!(
            r4os_sdk::subsystems::Launch::parse(launch.args())
                .unwrap()
                .guest()
                .unwrap(),
            guest
        );
        assert_eq!(
            path(launch.path()).unwrap().as_bytes(),
            b"C:\\R4OS\\SUBSYSTEMS\\r4os.snes\\R4SNES.R4X"
        );
    }
}
