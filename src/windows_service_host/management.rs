use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use windows_service::{
    service::{
        ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl,
        ServiceFailureActions, ServiceFailureResetPeriod, ServiceInfo, ServiceStartType,
        ServiceState,
    },
    service_manager::{ServiceManager, ServiceManagerAccess},
};

use super::{SERVICE_DESCRIPTION, SERVICE_DISPLAY_NAME, SERVICE_NAME, SERVICE_TYPE};

pub(super) fn install(config_path: &Path, should_start: bool) -> Result<()> {
    let config_path = absolute_config_path(config_path)?;
    if !config_path.is_file() {
        return Err(anyhow!(
            "Latitude config was not found at {}",
            config_path.display()
        ));
    }
    validate_service_config_security(&config_path)?;
    let executable = std::env::current_exe().context("Latitude executable path is unavailable")?;
    let info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(SERVICE_DISPLAY_NAME),
        service_type: SERVICE_TYPE,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: executable,
        launch_arguments: vec![
            OsString::from("--config"),
            config_path.clone().into_os_string(),
            OsString::from("service"),
            OsString::from("run"),
        ],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;
    let access = ServiceAccess::QUERY_STATUS
        | ServiceAccess::START
        | ServiceAccess::STOP
        | ServiceAccess::CHANGE_CONFIG;
    let service = match manager.open_service(SERVICE_NAME, access) {
        Ok(service) => {
            if service.query_status()?.current_state != ServiceState::Stopped {
                let _ = service.stop()?;
                wait_for_service_state(&service, ServiceState::Stopped)?;
            }
            service.change_config(&info)?;
            service
        }
        Err(_) => manager.create_service(&info, access)?,
    };
    service.set_description(SERVICE_DESCRIPTION)?;
    service.update_failure_actions(ServiceFailureActions {
        reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(60)),
        reboot_msg: None,
        command: None,
        actions: Some(vec![
            ServiceAction {
                action_type: ServiceActionType::Restart,
                delay: Duration::from_secs(2),
            },
            ServiceAction {
                action_type: ServiceActionType::Restart,
                delay: Duration::from_secs(5),
            },
        ]),
    })?;
    service.set_failure_actions_on_non_crash_failures(true)?;
    if should_start && service.query_status()?.current_state == ServiceState::Stopped {
        service.start::<&OsStr>(&[])?;
        wait_for_service_state(&service, ServiceState::Running)?;
    }
    println!(
        "Latitude service installed with config {}",
        config_path.display()
    );
    Ok(())
}

fn validate_service_config_security(config_path: &Path) -> Result<()> {
    let contents = std::fs::read_to_string(config_path).with_context(|| {
        format!(
            "Latitude service config could not be read from {}",
            config_path.display()
        )
    })?;
    let config: serde_json::Value = serde_json::from_str(&contents).with_context(|| {
        format!(
            "Latitude service config is not valid JSON: {}",
            config_path.display()
        )
    })?;
    let password = config
        .get("public_password")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("test");
    if password == "test" || password.trim().is_empty() {
        return Err(anyhow!(
            "refusing to install a LocalSystem service with the default public password; set a strong public_password in {}",
            config_path.display()
        ));
    }
    Ok(())
}

pub(super) fn uninstall() -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(
        SERVICE_NAME,
        ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
    )?;
    if service.query_status()?.current_state != ServiceState::Stopped {
        let _ = service.stop();
        wait_for_service_state(&service, ServiceState::Stopped)?;
    }
    service.delete()?;
    println!("Latitude service removed");
    Ok(())
}

pub(super) fn start() -> Result<()> {
    let service = open_service(ServiceAccess::QUERY_STATUS | ServiceAccess::START)?;
    if service.query_status()?.current_state == ServiceState::Stopped {
        service.start::<&OsStr>(&[])?;
    }
    wait_for_service_state(&service, ServiceState::Running)?;
    println!("Latitude service is running");
    Ok(())
}

pub(super) fn stop() -> Result<()> {
    let service = open_service(ServiceAccess::QUERY_STATUS | ServiceAccess::STOP)?;
    if service.query_status()?.current_state != ServiceState::Stopped {
        let _ = service.stop()?;
    }
    wait_for_service_state(&service, ServiceState::Stopped)?;
    println!("Latitude service is stopped");
    Ok(())
}

pub(super) fn status() -> Result<()> {
    let service = open_service(ServiceAccess::QUERY_STATUS)?;
    let state = format!("{:?}", service.query_status()?.current_state).to_ascii_lowercase();
    println!("{{\"name\":\"{SERVICE_NAME}\",\"state\":\"{state}\"}}");
    Ok(())
}

fn open_service(access: ServiceAccess) -> Result<windows_service::service::Service> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    manager
        .open_service(SERVICE_NAME, access)
        .map_err(Into::into)
}

fn wait_for_service_state(
    service: &windows_service::service::Service,
    expected: ServiceState,
) -> Result<()> {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let status = service.query_status()?;
        if status.current_state == expected {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(anyhow!(
                "timed out waiting for Latitude service state {expected:?}; current state is {:?}",
                status.current_state
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

pub(super) fn absolute_config_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(absolute)
}
