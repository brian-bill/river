pub mod ast;
pub mod lexer;
pub mod parser;

#[cfg(test)]
mod tests;

use std::fmt::Write;

use ariadne::{Color, Label, Report, ReportKind, Source};
use chumsky::Parser;
use crate::error::RiverError;

type Span = std::ops::Range<usize>;

fn span_to_line_col(input: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in input.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn report_chumsky_errors(
    input: &str,
    errors: &[chumsky::error::Simple<(lexer::Token, Span)>],
) -> (usize, String) {
    let mut output = String::new();
    let mut first_line = 0;

    for err in errors {
        let span = err.span();
        let (line, col) = span_to_line_col(input, span.start);
        if first_line == 0 {
            first_line = line;
        }

        let expected: Vec<_> = err
            .expected()
            .filter_map(|e| e.as_ref().map(|(tok, _)| tok.to_string()))
            .collect();
        let found = err
            .found()
            .map(|(tok, _)| tok.to_string())
            .unwrap_or_else(|| "end of input".to_string());

        let message = if !expected.is_empty() {
            format!("expected {} but found {}", expected.join(", "), found)
        } else {
            format!("unexpected {} at line {line}, column {col}", found)
        };

        let report = Report::build(ReportKind::Error, "river", span.start)
            .with_message(&message)
            .with_label(
                Label::new(("river", span.clone()))
                    .with_message("here")
                    .with_color(Color::Red),
            )
            .finish();

        let mut buf = Vec::new();
        report
            .write(("river", Source::from(input)), &mut buf)
            .ok();
        if let Ok(s) = String::from_utf8(buf) {
            let _ = writeln!(output, "{s}");
        }
    }

    (first_line, output)
}

pub fn parse(input: &str) -> Result<ast::Statement, RiverError> {
    let tokens: Vec<(lexer::Token, Span)> = lexer::lex(input)
        .into_iter()
        .map(|s| (s.token, s.span))
        .collect();

    let stmts = parser::parser().parse(tokens).map_err(|errors| {
        let (line, msg) = report_chumsky_errors(input, &errors);
        RiverError::Parse { line, msg }
    })?;

    Ok(stmts.into_iter().next().unwrap_or(ast::Statement::Noop))
}

/// Parse a RiverQL script into every statement it contains.
///
/// Unlike [`parse`], which returns only the first statement, this returns the
/// full `Vec<Statement>` so that file-processor scripts (`.rql`) can run several
/// statements separated by `;` in a single invocation. Empty input yields an
/// empty vector (the caller decides how to handle it).
pub fn parse_all(input: &str) -> Result<Vec<ast::Statement>, RiverError> {
    let tokens: Vec<(lexer::Token, Span)> = lexer::lex(input)
        .into_iter()
        .map(|s| (s.token, s.span))
        .collect();

    parser::parser().parse(tokens).map_err(|errors| {
        let (line, msg) = report_chumsky_errors(input, &errors);
        RiverError::Parse { line, msg }
    })
}
