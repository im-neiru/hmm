use std::panic::{AssertUnwindSafe, catch_unwind};

use burn::prelude::*;

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "android"))]
use burn::tensor::DeviceKind;
use burn::tensor::DeviceType;

use crate::errors::DeviceSelectError;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Backend {
    Cuda,
    Rocm,
    Vulkan,
    Metal,
    Cpu,
}

impl Backend {
    const ALL: [Backend; 5] = [
        Backend::Cuda,
        Backend::Rocm,
        Backend::Vulkan,
        Backend::Metal,
        Backend::Cpu,
    ];

    fn name(self) -> &'static str {
        match self {
            Backend::Cuda => "cuda",
            Backend::Rocm => "rocm",
            Backend::Vulkan => "vulkan",
            Backend::Metal => "metal",
            Backend::Cpu => "cpu",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.name() == name)
    }

    fn compiled(self) -> bool {
        match self {
            Backend::Cuda => cfg!(feature = "cuda"),
            Backend::Rocm => cfg!(feature = "rocm"),
            Backend::Vulkan => cfg!(any(
                target_os = "windows",
                target_os = "linux",
                target_os = "android"
            )),
            Backend::Metal => cfg!(any(target_os = "macos", target_os = "ios")),
            Backend::Cpu => true,
        }
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".into())
}

fn smoke_test(device: &Device) -> Result<(), String> {
    catch_unwind(AssertUnwindSafe(|| {
        let t = Tensor::<1>::from_floats([1.0, 2.0, 3.0], device);
        let sum: Vec<f32> = (t.clone() * t)
            .sum()
            .to_data()
            .try_into_vec()
            .map_err(|e| format!("readback failed: {e:?}"))?;

        if (sum[0] - 14.0).abs() > 1e-3 {
            return Err(format!("wrong result: expected 14, got {}", sum[0]));
        }

        let m = Tensor::<2>::from_floats([[1.0, 2.0], [3.0, 4.0]], device);
        let mm: Vec<f32> = m
            .clone()
            .matmul(m)
            .to_data()
            .try_into_vec()
            .map_err(|e| format!("readback failed: {e:?}"))?;

        if mm != [7.0, 10.0, 15.0, 22.0] {
            return Err(format!("wrong matmul result: {mm:?}"));
        }

        Ok(())
    }))
    .unwrap_or_else(|payload| Err(format!("panicked: {}", panic_message(payload))))
}

fn enumerate(filter: DeviceType, attempts: &mut Vec<String>, label: &str) -> Vec<Device> {
    match catch_unwind(AssertUnwindSafe(|| {
        Device::enumerate(filter)
            .into_iter()
            .collect::<Vec<Device>>()
    })) {
        Ok(devices) => {
            if devices.is_empty() {
                attempts.push(format!("{label}: no devices reported"));
            }
            devices
        }
        Err(payload) => {
            attempts.push(format!(
                "{label}: enumeration panicked: {}",
                panic_message(payload)
            ));
            Vec::new()
        }
    }
}

fn candidates(backend: Backend, attempts: &mut Vec<String>) -> Vec<(String, Device)> {
    let mut out: Vec<(String, Device)> = Vec::new();

    match backend {
        #[cfg(feature = "cuda")]
        Backend::Cuda => {
            for (i, d) in enumerate(DeviceType::Cuda, attempts, "cuda")
                .into_iter()
                .enumerate()
            {
                out.push((format!("cuda #{i} ({d:?})"), d));
            }
        }

        #[cfg(feature = "rocm")]
        Backend::Rocm => {
            for (i, d) in enumerate(DeviceType::Rocm, attempts, "rocm")
                .into_iter()
                .enumerate()
            {
                out.push((format!("rocm #{i} ({d:?})"), d));
            }
        }

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "android"))]
        Backend::Vulkan => {
            let found = enumerate(DeviceType::Vulkan, attempts, "vulkan");
            for (i, d) in found.iter().enumerate() {
                println!("[device] vulkan reports #{i}: {d:?}");
            }
            if found.is_empty() {
                return out;
            }

            for kind in [
                DeviceKind::DiscreteGpu(0),
                DeviceKind::IntegratedGpu(0),
                DeviceKind::VirtualGpu(0),
                DeviceKind::DefaultDevice,
            ] {
                let label = format!("vulkan {kind:?}");
                match catch_unwind(AssertUnwindSafe(|| Device::vulkan(kind.clone()))) {
                    Ok(device) => {
                        if !out.iter().any(|(_, d)| *d == device) {
                            out.push((format!("{label} ({device:?})"), device));
                        }
                    }
                    Err(payload) => {
                        attempts.push(format!(
                            "{label}: creation panicked: {}",
                            panic_message(payload)
                        ));
                    }
                }
            }
        }

        #[cfg(any(target_os = "macos", target_os = "ios"))]
        Backend::Metal => {
            for (i, d) in enumerate(DeviceType::Metal, attempts, "metal")
                .into_iter()
                .enumerate()
            {
                out.push((format!("metal #{i} ({d:?})"), d));
            }
        }

        Backend::Cpu => {
            let device = Device::cpu();
            out.push((format!("cpu ({device:?})"), device));
        }

        #[allow(unreachable_patterns)]
        _ => {}
    }

    out
}

pub fn select_device() -> Result<Device, DeviceSelectError> {
    let forced = std::env::var_os("HMM_BACKEND")
        .map(|v| v.to_string_lossy().trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty());

    let order: Vec<Backend> = match &forced {
        Some(name) => {
            let backend = Backend::from_name(name)
                .ok_or_else(|| DeviceSelectError::UnknownBackend(name.clone()))?;
            if !backend.compiled() {
                return Err(DeviceSelectError::BackendNotCompiled(name.clone()));
            }
            vec![backend]
        }
        None => Backend::ALL.into_iter().filter(|b| b.compiled()).collect(),
    };

    let mut attempts = Vec::new();

    for backend in order {
        for (label, device) in candidates(backend, &mut attempts) {
            match smoke_test(&device) {
                Ok(()) => {
                    println!("[device] selected {label}");
                    return Ok(device);
                }
                Err(reason) => {
                    println!("[device] rejected {label}: {reason}");
                    attempts.push(format!("{label}: {reason}"));
                }
            }
        }
    }

    Err(DeviceSelectError::NoWorkingDevice { forced, attempts })
}
