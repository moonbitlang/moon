use ariadne::Fmt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct MooncDiagnostic {
    pub level: String,
    #[serde(alias = "loc")]
    pub location: Location,
    pub message: String,
    pub error_code: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Location {
    pub start: Position,
    pub end: Position,
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Position {
    pub line: usize,
    pub col: usize,
    pub offset: isize,
}

impl MooncDiagnostic {
    pub fn render(content: &str, use_fancy: bool) {
        match serde_json_lenient::from_str::<MooncDiagnostic>(content) {
            Ok(diagnostic) => {
                let (kind, color) = diagnostic.get_level_and_color();

                // for no-location diagnostic, like Missing main function in the main package(4067)
                if diagnostic.location.path.is_empty() {
                    println!(
                        "{}",
                        format!(
                            "[{}] {}: {}",
                            diagnostic.error_code, kind, diagnostic.message
                        )
                        .fg(color)
                    );
                } else {
                    let source_file_path = &diagnostic.location.path;
                    let source_file = std::fs::read_to_string(source_file_path)
                        .unwrap_or_else(|_| panic!("failed to read {}", source_file_path));

                    let mut report_builder = ariadne::Report::build(
                        kind,
                        source_file_path,
                        diagnostic.location.start.offset as usize,
                    )
                    .with_code(diagnostic.error_code)
                    .with_message(&diagnostic.message)
                    .with_label(
                        ariadne::Label::new((
                            source_file_path,
                            diagnostic.location.start.offset as usize
                                ..diagnostic.location.end.offset as usize,
                        ))
                        .with_message((&diagnostic.message).fg(color))
                        .with_color(color),
                    );

                    if !use_fancy {
                        let config = ariadne::Config::default().with_color(false);
                        report_builder = report_builder.with_config(config);
                    }

                    report_builder
                        .finish()
                        .print((source_file_path, ariadne::Source::from(source_file)))
                        .unwrap();
                }
            }
            Err(_) => {
                println!("{}", content);
            }
        }
    }

    fn get_level_and_color(&self) -> (ariadne::ReportKind, ariadne::Color) {
        if self.level == "error" {
            (ariadne::ReportKind::Error, ariadne::Color::Red)
        } else if self.level == "warning" {
            (ariadne::ReportKind::Warning, ariadne::Color::BrightYellow)
        } else {
            (ariadne::ReportKind::Advice, ariadne::Color::Blue)
        }
    }
}
