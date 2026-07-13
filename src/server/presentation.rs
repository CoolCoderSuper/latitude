use crate::config::{
    ApplicationConfig, ApplicationTarget, PageFormat, ProjectConfig, is_binary_document_media_type,
};

pub(super) fn project_summary(project: &ProjectConfig) -> String {
    match enabled_deployment_count(project) {
        1 => "1 deployment".to_string(),
        count => format!("{count} deployments"),
    }
}

pub(super) fn enabled_deployment_count(project: &ProjectConfig) -> usize {
    project
        .deployments
        .iter()
        .filter(|deployment| deployment.enabled)
        .count()
}

pub(super) fn deployment_kind(deployment: &ApplicationConfig) -> &'static str {
    match &deployment.target {
        ApplicationTarget::ReverseProxy { .. } => "reverse_proxy",
        ApplicationTarget::Static { .. } => "static",
        ApplicationTarget::Page { .. } => "page",
    }
}

pub(super) fn deployment_home_label(deployment: &ApplicationConfig) -> &'static str {
    match &deployment.target {
        ApplicationTarget::ReverseProxy { .. } => "Website",
        ApplicationTarget::Static { index_file, .. } if is_static_image_deployment(index_file) => {
            "Image document"
        }
        ApplicationTarget::Static { index_file, .. } if is_static_video_deployment(index_file) => {
            "Video document"
        }
        ApplicationTarget::Static { .. } => "Static website",
        ApplicationTarget::Page {
            format: PageFormat::Binary,
            media_type,
            ..
        } if media_type.as_deref().is_some_and(is_image_media_type) => "Image document",
        ApplicationTarget::Page {
            format: PageFormat::Binary,
            media_type,
            ..
        } if media_type.as_deref().is_some_and(is_video_media_type) => "Video document",
        ApplicationTarget::Page {
            format: PageFormat::Binary,
            media_type,
            ..
        } if media_type
            .as_deref()
            .is_some_and(is_binary_document_media_type) =>
        {
            "Media document"
        }
        ApplicationTarget::Page { .. } => "Page",
    }
}

pub(super) fn deployment_page_title(deployment: &ApplicationConfig) -> Option<&str> {
    match &deployment.target {
        ApplicationTarget::Page { title, .. } => title.as_deref(),
        _ => None,
    }
}

pub(super) fn deployment_media_type(deployment: &ApplicationConfig) -> Option<String> {
    match &deployment.target {
        ApplicationTarget::Page {
            format: PageFormat::Binary,
            media_type,
            ..
        } => media_type.clone(),
        ApplicationTarget::Static { index_file, .. } => static_media_type(index_file),
        _ => None,
    }
}

fn static_media_type(index_file: &str) -> Option<String> {
    mime_guess::from_path(index_file)
        .first()
        .map(|mime| mime.essence_str().to_string())
        .filter(|media_type| is_binary_document_media_type(media_type))
}

fn is_static_image_deployment(index_file: &str) -> bool {
    static_media_type(index_file).is_some_and(|media_type| is_image_media_type(&media_type))
}

fn is_static_video_deployment(index_file: &str) -> bool {
    static_media_type(index_file).is_some_and(|media_type| is_video_media_type(&media_type))
}

fn is_image_media_type(media_type: &str) -> bool {
    media_type.starts_with("image/")
}

fn is_video_media_type(media_type: &str) -> bool {
    media_type.starts_with("video/")
}
