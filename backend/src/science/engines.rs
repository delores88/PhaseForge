use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

use crate::domain::ScientificEngineStatus;

pub fn discover_scientific_engines() -> Vec<ScientificEngineStatus> {
    vec![
        command_engine(
            "gromacs",
            "GROMACS",
            &["gmx", "gmx_mpi"],
            &["--version"],
            &["molecular dynamics", "free-energy workflows", "MPI ensembles"],
            "CUDA, HIP/SYCL, and MPI depend on the installed build",
            "Install a current GROMACS build and place gmx or gmx_mpi on PATH.",
        ),
        openmm_engine(),
        command_engine(
            "cp2k",
            "CP2K",
            &["cp2k", "cp2k.psmp", "cp2k.popt"],
            &["--version"],
            &["electronic structure", "molecular dynamics", "QM/MM"],
            "CUDA/HIP support depends on how CP2K was compiled",
            "Install CP2K and place a cp2k executable on PATH.",
        ),
        command_engine(
            "xtb",
            "xTB",
            &["xtb"],
            &["--version"],
            &["semiempirical quantum chemistry", "geometry optimization", "rapid rescoring"],
            "Primarily CPU; consult the installed build",
            "Install xTB and place xtb on PATH.",
        ),
        command_engine(
            "lammps",
            "LAMMPS",
            &["lmp", "lmp_mpi", "lammps"],
            &["-h"],
            &["particle dynamics", "materials simulation", "classical molecular dynamics"],
            "CUDA, HIP, SYCL, Kokkos, and MPI depend on the installed build",
            "Install LAMMPS and place lmp or lmp_mpi on PATH.",
        ),
        command_engine(
            "openbabel",
            "Open Babel",
            &["obabel"],
            &["-V"],
            &["structure conversion", "protonation helpers", "conformer preparation"],
            "CPU",
            "Install Open Babel and place obabel on PATH.",
        ),
    ]
}

fn openmm_engine() -> ScientificEngineStatus {
    let python = find_executable(&["python", "python3", "py"]);
    let version = python.as_ref().and_then(|path| {
        Command::new(path)
            .args(["-c", "import openmm; print(openmm.__version__)"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| first_line(&output.stdout))
    });
    ScientificEngineStatus {
        id: "openmm".to_owned(),
        name: "OpenMM".to_owned(),
        available: version.is_some(),
        executable: python.map(|path| path.display().to_string()),
        version,
        capabilities: vec![
            "molecular dynamics".to_owned(),
            "custom forces".to_owned(),
            "replicated trajectories".to_owned(),
        ],
        accelerator_support:
            "CPU, OpenCL, CUDA, and HIP depend on the installed OpenMM platforms".to_owned(),
        install_hint: "Install OpenMM in an isolated Python environment. PhaseForge detects it but does not silently change scientific environments."
            .to_owned(),
    }
}

fn command_engine(
    id: &str,
    name: &str,
    commands: &[&str],
    version_args: &[&str],
    capabilities: &[&str],
    accelerator_support: &str,
    install_hint: &str,
) -> ScientificEngineStatus {
    let executable = find_executable(commands);
    let version = executable.as_ref().and_then(|path| {
        Command::new(path)
            .args(version_args)
            .output()
            .ok()
            .filter(|output| output.status.success() || !output.stdout.is_empty() || !output.stderr.is_empty())
            .and_then(|output| first_line(&output.stdout).or_else(|| first_line(&output.stderr)))
    });
    ScientificEngineStatus {
        id: id.to_owned(),
        name: name.to_owned(),
        available: executable.is_some(),
        executable: executable.map(|path| path.display().to_string()),
        version,
        capabilities: capabilities.iter().map(|value| (*value).to_owned()).collect(),
        accelerator_support: accelerator_support.to_owned(),
        install_hint: install_hint.to_owned(),
    }
}

fn first_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(240).collect())
}

fn find_executable(candidates: &[&str]) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    let extensions: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    for directory in env::split_paths(&path) {
        for candidate in candidates {
            for extension in extensions {
                let path = directory.join(format!("{candidate}{extension}"));
                if is_executable_candidate(&path) {
                    return Some(path);
                }
            }
        }
    }
    None
}

fn is_executable_candidate(path: &Path) -> bool {
    path.is_file()
}
