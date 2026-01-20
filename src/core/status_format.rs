pub struct Row {
    pub label: &'static str,
    pub value: String,
}

pub fn write_rows(
    f: &mut std::fmt::Formatter<'_>,
    indent: &str,
    gap: &str,
    rows: &[Row],
) -> std::fmt::Result {
    let width = rows.iter().map(|row| row.label.len()).max().unwrap_or(0);
    for row in rows {
        writeln!(
            f,
            "{indent}{label:<width$}{gap}{value}",
            label = row.label,
            value = row.value
        )?;
    }
    Ok(())
}
