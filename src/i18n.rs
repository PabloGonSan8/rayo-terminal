//! UI language. Spanish is the source text; English comes from `EN`.
//! Chosen in Settings ("auto" = Windows UI language; `RAYO_LANG=en|es` overrides auto).

use std::sync::atomic::{AtomicBool, Ordering};

static EN_ON: AtomicBool = AtomicBool::new(false);

pub fn en() -> bool {
    EN_ON.load(Ordering::Relaxed)
}

/// Apply the `language` setting: "es", "en" or anything else for automatic.
pub fn set(choice: &str) {
    let en = match choice {
        "es" => false,
        "en" => true,
        _ => match std::env::var("RAYO_LANG") {
            Ok(l) => !l.to_lowercase().starts_with("es"),
            Err(_) => !system_is_spanish(),
        },
    };
    EN_ON.store(en, Ordering::Relaxed);
}

#[cfg(windows)]
fn system_is_spanish() -> bool {
    // Primary language id = low 10 bits of the LANGID; LANG_SPANISH = 0x0a.
    let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    id & 0x3ff == 0x0a
}

#[cfg(not(windows))]
fn system_is_spanish() -> bool {
    std::env::var("LANG").is_ok_and(|l| l.starts_with("es"))
}

/// Spanish source text → current language. Unknown text comes back unchanged.
/// ponytail: linear scan of ~130 pairs, only for on-screen labels; a HashMap if it ever shows in a profile.
pub fn t(es: &str) -> &str {
    if !en() {
        return es;
    }
    lookup(es)
}

fn lookup(es: &str) -> &str {
    EN.iter().find(|(k, _)| *k == es).map_or(es, |(_, v)| v)
}

/// `es` or `en` by the current language, for text built with `format!`.
pub fn pick<'a>(es: &'a str, en_text: &'a str) -> &'a str {
    if en() { en_text } else { es }
}

const EN: &[(&str, &str)] = &[
    // Settings categories
    ("Apariencia", "Appearance"),
    ("Idioma", "Language"),
    ("Automático", "Automatic"),
    ("Atajos", "Shortcuts"),
    // Fixed keys
    ("Ejecutar comando", "Run command"),
    ("Nueva línea en el editor", "New line in the editor"),
    ("Historial de comandos", "Command history"),
    ("Autocompletar ruta", "Complete path"),
    ("Aceptar sugerencia", "Accept suggestion"),
    ("Interrumpir proceso / borrar línea", "Interrupt process / clear line"),
    ("Copiar · Pegar", "Copy · Paste"),
    ("Ctrl C · Ctrl V · clic derecho", "Ctrl C · Ctrl V · right click"),
    ("Ir a pestaña N", "Go to tab N"),
    ("Renombrar pestaña", "Rename tab"),
    ("Doble clic en la pestaña", "Double click the tab"),
    ("Cerrar pestaña", "Close tab"),
    ("Clic central en la pestaña", "Middle click the tab"),
    // Top bar / panes
    ("Nueva pestaña  (Ctrl Shift T)", "New tab  (Ctrl Shift T)"),
    ("Ajustes  (Ctrl ,)", "Settings  (Ctrl ,)"),
    ("Agentes de IA", "AI agents"),
    ("Dividir hacia abajo  (Ctrl Shift E)", "Split down  (Ctrl Shift E)"),
    ("Dividir a la derecha  (Ctrl Shift D)", "Split right  (Ctrl Shift D)"),
    (
        "Instala la nueva versión y reinicia Rayo (las pestañas se restauran)",
        "Installs the new version and restarts Rayo (tabs are restored)",
    ),
    ("Cerrar panel  (Ctrl Shift W)", "Close pane  (Ctrl Shift W)"),
    (
        "Ctrl C, Ctrl V y Ctrl X están reservados para copiar, pegar y cortar.",
        "Ctrl C, Ctrl V and Ctrl X are reserved for copy, paste and cut.",
    ),
    // Settings window
    ("Ajustes", "Settings"),
    ("Cerrar  (Esc)", "Close  (Esc)"),
    ("Buscar ajustes…", "Search settings…"),
    ("Ningún ajuste coincide", "No setting matches"),
    ("Se guarda automáticamente", "Saved automatically"),
    ("Abrir config.toml", "Open config.toml"),
    ("Tema", "Theme"),
    ("Colores del terminal y de la interfaz", "Terminal and interface colors"),
    ("Texto de salida", "Output text"),
    ("Lo que escriben los programas", "What programs print"),
    ("Comandos", "Commands"),
    ("El comando de cada bloque", "Each block's command"),
    ("Texto que escribes", "Text you type"),
    ("El editor de abajo", "The editor at the bottom"),
    ("Ruta (carpeta)", "Path (folder)"),
    ("Chip de la carpeta actual", "Current folder chip"),
    ("Rama git", "Git branch"),
    ("Chip de la rama", "Branch chip"),
    ("Texto secundario", "Secondary text"),
    ("«Ejecutando», «Interrumpido» y ayudas", "\"Running\", \"Interrupted\" and hints"),
    ("Letra del editor", "Editor font size"),
    ("Tamaño del texto que escribes abajo", "Size of the text you type at the bottom"),
    ("Altura del editor", "Editor height"),
    ("Líneas visibles como mínimo", "Minimum visible lines"),
    ("Tema según la hora", "Theme by time of day"),
    (
        "Usa un tema de día y otro de noche automáticamente",
        "Switches between a day and a night theme automatically",
    ),
    ("Tema de día", "Day theme"),
    ("Tema de noche", "Night theme"),
    ("Empieza el día", "Day starts"),
    ("Empieza la noche", "Night starts"),
    ("Hora local", "Local time"),
    (
        "Paleta ANSI del tema: los 16 colores que usan los programas para colorear su salida (errores, git, ls…). Pasa el ratón para ver cada uno.",
        "Theme ANSI palette: the 16 colors programs use to color their output (errors, git, ls…). Hover to see each one.",
    ),
    (" brillante", " bright"),
    ("Negro", "Black"),
    ("Rojo", "Red"),
    ("Verde", "Green"),
    ("Amarillo", "Yellow"),
    ("Azul", "Blue"),
    ("Cian", "Cyan"),
    ("Blanco", "White"),
    ("Fuente", "Font"),
    ("Fuentes monoespaciadas instaladas", "Installed monospace fonts"),
    ("Tamaño de fuente", "Font size"),
    ("También con Ctrl = / Ctrl -", "Also with Ctrl = / Ctrl -"),
    ("Peso de fuente", "Font weight"),
    ("Grosor del trazo (fuentes variables como Cascadia)", "Stroke weight (variable fonts like Cascadia)"),
    ("Altura de línea", "Line height"),
    ("Espacio vertical entre líneas", "Vertical space between lines"),
    ("Margen interior", "Padding"),
    ("Espacio entre el borde del panel y el texto", "Space between the pane edge and the text"),
    ("Opacidad del fondo", "Background opacity"),
    ("Transparencia de la ventana", "Window transparency"),
    ("Atenuar paneles inactivos", "Dim inactive panes"),
    ("Oscurece los paneles divididos sin foco", "Darkens split panes without focus"),
    ("Bloques de comandos", "Command blocks"),
    (
        "Historial de bloques y editor propio (PowerShell). Terminales nuevas",
        "Block history and built-in editor (PowerShell). New terminals",
    ),
    ("Barra «Ejecutando»", "\"Running\" bar"),
    (
        "Barra inferior mientras corre un comando. Desactivada: el proceso ocupa todo el panel",
        "Bottom bar while a command runs. Off: the process fills the whole pane",
    ),
    ("Restaurar sesión al abrir", "Restore session on start"),
    (
        "Vuelve a abrir las pestañas, divisiones y carpetas de la última vez",
        "Reopens last time's tabs, splits and folders",
    ),
    ("Bloques en historial", "Blocks in history"),
    ("Máximo de bloques guardados por panel", "Maximum blocks kept per pane"),
    ("Estilo del cursor", "Cursor style"),
    ("Subrayado", "Underline"),
    ("Barra", "Bar"),
    ("Bloque", "Block"),
    ("Cursor parpadeante", "Blinking cursor"),
    ("Líneas de historial", "Scrollback lines"),
    ("Scrollback. Se aplica a terminales nuevas", "Applies to new terminals"),
    ("Copiar al seleccionar", "Copy on select"),
    ("Copia al soltar el ratón", "Copies when the mouse is released"),
    ("Clic derecho pega", "Right click pastes"),
    ("Con selección, el clic derecho copia", "With a selection, right click copies"),
    ("Shell predeterminado", "Default shell"),
    ("Para pestañas y paneles nuevos", "For new tabs and panes"),
    ("Personalizado", "Custom"),
    ("Programa", "Program"),
    ("Ruta o nombre del ejecutable", "Path or name of the executable"),
    ("Argumentos", "Arguments"),
    ("Separados por espacios", "Separated by spaces"),
    ("Directorio inicial", "Start directory"),
    ("Vacío = carpeta de usuario", "Empty = user folder"),
    ("Símbolo del sistema (CMD)", "Command Prompt (CMD)"),
    (
        "Haz clic en un atajo y pulsa la nueva combinación. Supr lo deja sin asignar, Esc cancela.",
        "Click a shortcut and press the new combination. Del unassigns it, Esc cancels.",
    ),
    ("Pulsa una combinación…", "Press a combination…"),
    ("Sin asignar", "Unassigned"),
    ("Cambiar atajo", "Change shortcut"),
    ("Restablecer", "Reset"),
    ("Fijo", "Fixed"),
    ("Restablecer todos los atajos", "Reset all shortcuts"),
    // Rebindable actions (keys.rs)
    ("Abrir ajustes", "Open settings"),
    ("Nueva pestaña", "New tab"),
    ("Cerrar panel / pestaña", "Close pane / tab"),
    ("Dividir a la derecha", "Split right"),
    ("Dividir hacia abajo", "Split down"),
    ("Maximizar / restaurar panel", "Maximize / restore pane"),
    ("Siguiente pestaña", "Next tab"),
    ("Pestaña anterior", "Previous tab"),
    ("Foco al panel izquierdo", "Focus left pane"),
    ("Foco al panel derecho", "Focus right pane"),
    ("Foco al panel superior", "Focus pane above"),
    ("Foco al panel inferior", "Focus pane below"),
    ("Copiar selección", "Copy selection"),
    ("Pegar", "Paste"),
    ("Aumentar letra", "Bigger font"),
    ("Reducir letra", "Smaller font"),
    ("Tamaño de letra original", "Reset font size"),
    ("Limpiar bloques", "Clear blocks"),
    ("Subir una página del historial", "Scroll up one page"),
    ("Bajar una página del historial", "Scroll down one page"),
    ("RePág", "PgUp"),
    ("AvPág", "PgDn"),
    // Agents popup
    ("Agentes activos", "Active agents"),
    ("Ninguno en ejecución", "None running"),
    ("Ir al panel", "Go to pane"),
    ("Uso de Claude Code", "Claude Code usage"),
    ("Calculando…", "Calculating…"),
    ("Últimos 7 días", "Last 7 days"),
    ("Sesión actual (5 h)", "Current session (5 h)"),
    ("Por modelo (7 días)", "By model (7 days)"),
    ("Aproximado · solo sesiones de este equipo", "Approximate · sessions on this computer only"),
    // Blocks / editor
    ("Carpeta", "Folder"),
    ("Archivo", "File"),
    ("Sin subcarpetas", "No subfolders"),
    ("Ctrl+C interrumpe · el teclado va al proceso", "Ctrl+C interrupts · keys go to the process"),
    ("Iniciando PowerShell…", "Starting PowerShell…"),
    ("Escribe un comando...", "Type a command..."),
    ("Copiar comando", "Copy command"),
    ("Copiar salida", "Copy output"),
    ("Copiar comando y salida", "Copy command and output"),
    ("Editar de nuevo", "Edit again"),
    ("Ejecutando…", "Running…"),
    ("No se pudo descargar la actualización", "Couldn't download the update"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_translates_known_and_keeps_unknown_text() {
        assert_eq!(lookup("Nueva pestaña"), "New tab");
        assert_eq!(lookup("cargo build"), "cargo build");
    }

    #[test]
    fn table_has_no_duplicate_keys() {
        let mut keys: Vec<_> = EN.iter().map(|(k, _)| *k).collect();
        keys.sort_unstable();
        let n = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), n);
    }
}
