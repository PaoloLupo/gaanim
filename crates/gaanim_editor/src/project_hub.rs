use bevy::prelude::*;
use bevy_egui::{EguiPrimaryContextPass, egui};
use gaanim_project::{
    CreateProjectOptions, EnvironmentProbe, ProjectKind, RecentProjects, ResolvedProject,
    create_project, default_project_parent, resolve_project,
};
use std::path::{Component, Path, PathBuf};

pub struct ProjectHubPlugin;

impl Plugin for ProjectHubPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ProjectHubState>()
            .init_resource::<PendingProjectOpen>()
            .add_systems(EguiPrimaryContextPass, project_hub_ui_system);
    }
}

#[derive(Resource, Default)]
pub struct PendingProjectOpen(pub Option<ResolvedProject>);

#[derive(Debug)]
enum HubPage {
    Home,
    Create,
    Review {
        project: Box<ResolvedProject>,
        probe: EnvironmentProbe,
    },
}

#[derive(Resource)]
pub struct ProjectHubState {
    pub active: bool,
    page: HubPage,
    recents: RecentProjects,
    project_name: String,
    parent: PathBuf,
    kind: ProjectKind,
    error: Option<String>,
    global_probe: EnvironmentProbe,
}

impl Default for ProjectHubState {
    fn default() -> Self {
        Self {
            active: false,
            page: HubPage::Home,
            recents: RecentProjects::load(),
            project_name: String::new(),
            parent: default_project_parent(),
            kind: ProjectKind::Video,
            error: None,
            global_probe: EnvironmentProbe::detect(None),
        }
    }
}

impl ProjectHubState {
    pub fn show(&mut self) {
        self.active = true;
        self.global_probe = EnvironmentProbe::detect(None);
    }

    pub fn report_open_error(&mut self, error: String) {
        self.active = true;
        self.error = Some(error);
    }

    fn prepare_project(&mut self, project: ResolvedProject, pending: &mut PendingProjectOpen) {
        self.error = None;
        self.recents.record(&project);
        let _ = self.recents.save();
        let probe = EnvironmentProbe::detect(Some(&project.root));
        if probe.has_supported_python() && probe.has_venv() {
            pending.0 = Some(project);
        } else {
            self.page = HubPage::Review {
                project: Box::new(project),
                probe,
            };
        }
    }
}

fn project_hub_ui_system(
    mut contexts: bevy_egui::EguiContexts,
    mut state: ResMut<ProjectHubState>,
    mut pending: ResMut<PendingProjectOpen>,
) {
    if !state.active {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(egui::Color32::from_rgb(15, 18, 28)))
        .show(ctx, |ui| {
            ui.add_space(28.0);
            ui.vertical_centered(|ui| {
                ui.heading(
                    egui::RichText::new("Gaanim")
                        .size(36.0)
                        .color(egui::Color32::from_rgb(132, 145, 255)),
                );
                ui.label(
                    egui::RichText::new("Crea y abre animaciones vectoriales")
                        .size(16.0)
                        .color(egui::Color32::LIGHT_GRAY),
                );
            });
            ui.add_space(24.0);
            let page = std::mem::replace(&mut state.page, HubPage::Home);
            match page {
                HubPage::Home => home_page(ui, &mut state, &mut pending),
                HubPage::Create => {
                    state.page = HubPage::Create;
                    create_page(ui, &mut state, &mut pending);
                }
                HubPage::Review { project, mut probe } => {
                    let stay = review_page(ui, &project, &mut probe, &mut pending);
                    if stay && pending.0.is_none() {
                        state.page = HubPage::Review { project, probe };
                    }
                }
            }
            if let Some(error) = &state.error {
                ui.add_space(12.0);
                ui.colored_label(egui::Color32::from_rgb(255, 120, 120), error);
            }
        });
}

fn home_page(ui: &mut egui::Ui, state: &mut ProjectHubState, pending: &mut PendingProjectOpen) {
    ui.horizontal(|ui| {
        if ui.button("＋ Nuevo proyecto").clicked() {
            state.error = None;
            state.page = HubPage::Create;
        }
        if ui.button("Abrir proyecto…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .set_title("Abrir proyecto Gaanim")
                .pick_folder()
        {
            match resolve_project(&path) {
                Ok(project) => state.prepare_project(project, pending),
                Err(error) => state.error = Some(error),
            }
        }
        if ui.button("Volver a comprobar entorno").clicked() {
            state.global_probe = EnvironmentProbe::detect(None);
        }
    });
    ui.add_space(12.0);
    environment_badge(ui, &state.global_probe);
    ui.add_space(24.0);
    ui.horizontal(|ui| {
        ui.heading("Proyectos recientes");
        if !state.recents.projects().is_empty() && ui.small_button("Limpiar").clicked() {
            state.recents.clear();
            let _ = state.recents.save();
        }
    });
    ui.separator();
    let projects = state.recents.projects();
    if projects.is_empty() {
        ui.add_space(18.0);
        ui.label("Todavía no hay proyectos recientes.");
        return;
    }
    let mut open = None;
    let mut remove = None;
    for project in projects {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(
                        false,
                        format!(
                            "{}  ·  {}",
                            project.manifest.name,
                            project.manifest.kind.name()
                        ),
                    )
                    .clicked()
                {
                    open = Some(project.clone());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Quitar").clicked() {
                        remove = Some(project.root.clone());
                    }
                });
            });
            ui.small(project.root.display().to_string());
        });
        ui.add_space(6.0);
    }
    if let Some(root) = remove {
        state.recents.remove(&root);
        let _ = state.recents.save();
    }
    if let Some(project) = open {
        state.prepare_project(project, pending);
    }
}

fn create_page(ui: &mut egui::Ui, state: &mut ProjectHubState, pending: &mut PendingProjectOpen) {
    ui.horizontal(|ui| {
        if ui.button("← Volver").clicked() {
            state.page = HubPage::Home;
            state.error = None;
        }
        ui.heading("Nuevo proyecto");
    });
    ui.add_space(16.0);
    ui.label("Tipo");
    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.kind, ProjectKind::Video, "Video");
        ui.selectable_value(&mut state.kind, ProjectKind::Slides, "Slides");
    });
    ui.add_space(10.0);
    ui.label("Nombre");
    ui.text_edit_singleline(&mut state.project_name);
    ui.add_space(10.0);
    ui.label("Ubicación");
    ui.horizontal(|ui| {
        ui.monospace(state.parent.display().to_string());
        if ui.button("Cambiar…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .set_title("Ubicación del proyecto")
                .set_directory(&state.parent)
                .pick_folder()
        {
            state.parent = path;
        }
    });
    let destination = state.parent.join(state.project_name.trim());
    ui.small(format!("Destino: {}", destination.display()));
    ui.add_space(16.0);
    if ui.button("Crear proyecto").clicked() {
        state.error = None;
        if !valid_project_name(state.project_name.trim()) {
            state.error = Some("El nombre debe ser un único nombre de carpeta no vacío.".into());
            return;
        }
        match create_project(&CreateProjectOptions {
            kind: state.kind,
            directory: destination,
            force: false,
        }) {
            Ok(project) => {
                state.project_name.clear();
                state.prepare_project(project, pending);
            }
            Err(error) => state.error = Some(error),
        }
    }
}

fn valid_project_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn review_page(
    ui: &mut egui::Ui,
    project: &ResolvedProject,
    probe: &mut EnvironmentProbe,
    pending: &mut PendingProjectOpen,
) -> bool {
    ui.heading("Revisar entorno");
    ui.label(format!("Proyecto: {}", project.manifest.name));
    ui.monospace(project.root.display().to_string());
    ui.add_space(14.0);
    environment_badge(ui, probe);
    ui.add_space(12.0);
    let command = uv_command_for(&project.root, probe);
    ui.label(if probe.uv.is_some() {
        "Gaanim puede crear .venv e instalar su paquete de autocompletado:"
    } else {
        "Instala uv y luego crea el entorno fuera de Gaanim:"
    });
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.monospace(&command);
        if ui.small_button("Copiar instrucciones").clicked() {
            ui.ctx().copy_text(command.clone());
        }
    });
    ui.add_space(12.0);
    let mut stay = true;
    ui.horizontal(|ui| {
        if ui.button("← Proyectos").clicked() {
            stay = false;
        }
        if ui.button("Volver a comprobar").clicked() {
            *probe = EnvironmentProbe::detect(Some(&project.root));
        }
        if ui
            .add_enabled(
                probe.has_supported_python() || probe.uv.is_some(),
                egui::Button::new("Preparar y abrir"),
            )
            .clicked()
        {
            pending.0 = Some(project.clone());
        }
    });
    if !probe.has_supported_python() && probe.uv.is_none() {
        ui.colored_label(
            egui::Color32::from_rgb(255, 170, 90),
            "Instala uv para que Gaanim prepare Python 3.12 y el autocompletado.",
        );
    }
    stay
}

fn environment_badge(ui: &mut egui::Ui, probe: &EnvironmentProbe) {
    match &probe.python {
        Some(python) if python.version.is_supported() => {
            let isolation = if python.venv_root.is_some() {
                "entorno aislado"
            } else {
                "Python del sistema · sin aislamiento"
            };
            ui.colored_label(
                egui::Color32::from_rgb(120, 220, 150),
                format!(
                    "Python {}.{}.{} · {isolation}",
                    python.version.major, python.version.minor, python.version.patch
                ),
            );
        }
        Some(python) => {
            ui.colored_label(
                egui::Color32::from_rgb(255, 170, 90),
                format!(
                    "Python {}.{} no es compatible",
                    python.version.major, python.version.minor
                ),
            );
        }
        None => {
            ui.colored_label(
                egui::Color32::from_rgb(255, 120, 120),
                "Python compatible no encontrado",
            );
        }
    }
    ui.small(match &probe.uv {
        Some(uv) => format!("uv detectado: {}", uv.version),
        None => "uv no está instalado".to_string(),
    });
}

fn uv_command_for(root: &Path, probe: &EnvironmentProbe) -> String {
    if cfg!(windows) {
        let escaped = root.display().to_string().replace('\'', "''");
        let setup = if probe.uv.is_some() {
            String::new()
        } else {
            "winget install --id=astral-sh.uv -e\n".to_string()
        };
        format!(
            "{setup}Set-Location -LiteralPath '{escaped}'\n# Gaanim creará .venv e instalará el wheel al abrir"
        )
    } else {
        let escaped = root.display().to_string().replace('\'', "'\\''");
        let setup = if probe.uv.is_some() {
            String::new()
        } else {
            "curl -LsSf https://astral.sh/uv/install.sh | sh\n".to_string()
        };
        format!("{setup}cd '{escaped}'\n# Gaanim creará .venv e instalará el wheel al abrir")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_name_is_one_normal_component() {
        assert!(valid_project_name("demo"));
        assert!(!valid_project_name(""));
        assert!(!valid_project_name("../demo"));
        assert!(!valid_project_name("a/b"));
    }

    #[test]
    fn uv_instructions_explain_automatic_environment_setup() {
        let command = uv_command_for(Path::new("demo"), &EnvironmentProbe::default());
        assert!(command.contains("Gaanim creará .venv"));
        assert!(!command.contains("pip install"));
    }
}
