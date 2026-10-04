use std::{env, path::PathBuf, process::Command};

fn run(command: &mut Command) {
    let status = command
        .status()
        .expect("Could not launch CMake; install CMake and a C++20 compiler");
    assert!(
        status.success(),
        "Pinned native geometry build failed: {command:?}"
    );
}

fn main() {
    for path in ["cpp", "vendor", "build.rs"] {
        println!("cargo:rerun-if-changed={path}");
    }
    for key in [
        "CMAKE",
        "CXX",
        "CC",
        "CMAKE_GENERATOR",
        "MACOSX_DEPLOYMENT_TARGET",
        "RESHIKI_GEOMETRY_NATIVE_TESTS",
        "CTEST",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let build = out.join("native-build");
    let install = out.join("native-install");
    let target = env::var("TARGET").unwrap();
    let host = env::var("HOST").unwrap();
    let cmake = env::var_os("CMAKE").unwrap_or_else(|| "cmake".into());
    let native_tests = env::var("RESHIKI_GEOMETRY_NATIVE_TESTS").is_ok_and(|value| value == "1");
    let mut configure = Command::new(&cmake);
    configure
        .arg("-S")
        .arg(root.join("cpp"))
        .arg("-B")
        .arg(&build)
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg(format!("-DCMAKE_INSTALL_PREFIX={}", install.display()));
    configure.arg(if native_tests {
        "-DRESHIKI_GEOMETRY_BUILD_TESTS=ON"
    } else {
        "-DRESHIKI_GEOMETRY_BUILD_TESTS=OFF"
    });
    if target.contains("windows-msvc") {
        // A Visual Studio generator builds for the requested architecture even
        // when the host is x64. CRT linkage must match the Rust target setting.
        let generator = env::var("CMAKE_GENERATOR").unwrap_or_default();
        if generator.is_empty() || generator.starts_with("Visual Studio") {
            configure.arg("-A").arg(if target.starts_with("aarch64") {
                "ARM64"
            } else {
                "x64"
            });
        }
        let static_crt = env::var("CARGO_CFG_TARGET_FEATURE")
            .unwrap_or_default()
            .split(',')
            .any(|feature| feature == "crt-static");
        configure.arg(if static_crt {
            "-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded"
        } else {
            "-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL"
        });
    } else {
        for prefix in ["CXX", "CC"] {
            let suffix = target.replace('-', "_");
            let key = format!("{prefix}_{suffix}");
            println!("cargo:rerun-if-env-changed={key}");
            if let Some(compiler) = env::var_os(&key).or_else(|| env::var_os(prefix)) {
                configure.arg(format!(
                    "-DCMAKE_{}_COMPILER={}",
                    if prefix == "CXX" { "CXX" } else { "C" },
                    compiler.to_string_lossy()
                ));
            }
        }
    }
    if target.contains("apple-darwin") {
        configure.arg(format!(
            "-DCMAKE_OSX_ARCHITECTURES={}",
            if target.starts_with("aarch64") {
                "arm64"
            } else {
                "x86_64"
            }
        ));
        let deployment = env::var("MACOSX_DEPLOYMENT_TARGET").unwrap_or_else(|_| "14.0".into());
        configure.arg(format!("-DCMAKE_OSX_DEPLOYMENT_TARGET={deployment}"));
    } else if target != host && !target.contains("windows-msvc") {
        panic!(
            "Build Linux geometry on its native architecture; no implicit cross toolchain is selected"
        );
    }
    run(&mut configure);
    let jobs = env::var("NUM_JOBS")
        .ok()
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(2)
        .clamp(1, 4)
        .to_string();
    run(Command::new(&cmake)
        .arg("--build")
        .arg(&build)
        .arg("--config")
        .arg("Release")
        .arg("--target")
        .arg("install")
        .arg("--parallel")
        .arg(jobs));
    if native_tests {
        assert_eq!(
            host, target,
            "Native geometry checks must run on their target architecture"
        );
        // CMake supplies a matching CTest beside its executable on all release
        // runners; a custom CMAKE may still select CTEST explicitly.
        let ctest = env::var_os("CTEST").unwrap_or_else(|| {
            let beside = PathBuf::from(&cmake).with_file_name(if target.contains("windows") {
                "ctest.exe"
            } else {
                "ctest"
            });
            if beside.is_file() {
                beside.into_os_string()
            } else {
                "ctest".into()
            }
        });
        run(Command::new(ctest)
            .arg("--test-dir")
            .arg(&build)
            .arg("-C")
            .arg("Release")
            .arg("--output-on-failure")
            .arg("--timeout")
            .arg("90"));
    }
    println!(
        "cargo:rustc-link-search=native={}",
        install.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=reshiki_geometry");
    if target.contains("apple-darwin") {
        println!("cargo:rustc-link-lib=c++");
    } else if target.contains("linux") {
        println!("cargo:rustc-link-lib=stdc++");
        println!("cargo:rustc-link-lib=pthread");
        println!("cargo:rustc-link-lib=m");
    }
}
