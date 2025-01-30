pub fn build_globset(glob_strings: &[String]) -> Result<globset::GlobSet, anyhow::Error> {
    let mut builder = globset::GlobSetBuilder::new();

    for glob_str in glob_strings.iter() {
        let glob = globset::Glob::new(glob_str)?;
        builder.add(glob);
    }

    let globset = builder.build()?;

    Ok(globset)
}
