use crate::error::{AppError, Result};
use std::path::Path;

/// Parse a file at `path` into plain text. Supports .pdf, .docx, .md, .txt.
pub fn parse(path: &Path) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "pdf" => parse_pdf(path),
        "docx" => parse_docx(path),
        "md" | "txt" => parse_text(path),
        other => Err(AppError::Config(format!(
            "unsupported file extension: {other}"
        ))),
    }
}

fn parse_pdf(path: &Path) -> Result<String> {
    pdf_extract::extract_text(path)
        .map_err(|e| AppError::Config(format!("pdf parse failed: {e}")))
}

fn parse_docx(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let docx = docx_rs::read_docx(&bytes)
        .map_err(|e| AppError::Config(format!("docx parse failed: {e:?}")))?;

    let mut text = String::new();
    for child in &docx.document.children {
        extract_doc_child(child, &mut text);
    }
    Ok(text)
}

fn extract_doc_child(child: &docx_rs::DocumentChild, out: &mut String) {
    use docx_rs::DocumentChild;
    match child {
        DocumentChild::Paragraph(p) => {
            extract_paragraph(p, out);
            out.push('\n');
        }
        DocumentChild::Table(t) => {
            for row in &t.rows {
                let docx_rs::TableChild::TableRow(row) = row;
                for cell in &row.cells {
                    let docx_rs::TableRowChild::TableCell(cell) = cell;
                    for c in &cell.children {
                        if let docx_rs::TableCellContent::Paragraph(p) = c {
                            extract_paragraph(p, out);
                            out.push('\t');
                        }
                    }
                }
                out.push('\n');
            }
        }
        _ => {}
    }
}

fn extract_paragraph(p: &docx_rs::Paragraph, out: &mut String) {
    use docx_rs::{ParagraphChild, RunChild};
    for child in &p.children {
        if let ParagraphChild::Run(run) = child {
            for rc in &run.children {
                if let RunChild::Text(t) = rc {
                    out.push_str(&t.text);
                }
            }
        }
    }
}

fn parse_text(path: &Path) -> Result<String> {
    Ok(std::fs::read_to_string(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp(content: &str, suffix: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("test.{suffix}"));
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        (dir, path)
    }

    #[test]
    fn parses_txt() {
        let (_dir, path) = write_temp("hello\n中文测试\n", "txt");
        let text = parse(&path).unwrap();
        assert!(text.contains("hello"));
        assert!(text.contains("中文测试"));
    }

    #[test]
    fn parses_md() {
        let (_dir, path) = write_temp("# Heading\n\n陆家嘴报价 211 万。", "md");
        let text = parse(&path).unwrap();
        assert!(text.contains("陆家嘴"));
        assert!(text.contains("211"));
    }

    #[test]
    fn rejects_unsupported_extension() {
        let (_dir, path) = write_temp("data", "xyz");
        let err = parse(&path).unwrap_err();
        assert!(err.to_string().contains("unsupported"));
    }

    #[test]
    fn parses_docx_generated() {
        // Generate a small docx fixture programmatically using docx-rs itself,
        // then parse it back — verifies the full round-trip including Chinese chars.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.docx");

        use docx_rs::*;
        let docx = Docx::new()
            .add_paragraph(
                Paragraph::new().add_run(Run::new().add_text("陆家嘴连桥项目报价 211 万")),
            )
            .add_paragraph(
                Paragraph::new().add_run(Run::new().add_text("EFC 幕墙顾问服务范围 8 个阶段")),
            );
        let file = std::fs::File::create(&path).unwrap();
        docx.build().pack(file).unwrap();

        let text = parse(&path).unwrap();
        assert!(text.contains("陆家嘴"), "expected 陆家嘴 in: {text}");
        assert!(text.contains("EFC"), "expected EFC in: {text}");
    }

    #[test]
    #[ignore = "requires manually-placed PDF fixture (no easy programmatic gen for Chinese)"]
    fn parses_pdf_fixture() {
        let path = std::path::Path::new("../tests/fixtures/test.pdf");
        if !path.exists() {
            panic!(
                "missing test fixture at {path:?}; generate with `pandoc -o test.pdf input.md` or skip"
            );
        }
        let text = parse(path).unwrap();
        assert!(!text.is_empty(), "extracted text should be non-empty");
    }
}
