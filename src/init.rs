use std::fs::{self, File};
use std::io::Write;

/// Initialize an empty Rocket project with the given name.
pub fn init(name: &str) {
    debug!("Creating directory '{}'", name);
    fs::DirBuilder::new()
        .create(name)
        .expect("Failed to create directory");
    fs::DirBuilder::new()
        .create(format!("{}/content", name))
        .expect("Failed to create directory");
    fs::DirBuilder::new()
        .create(format!("{}/theme", name))
        .expect("Failed to create directory");

    let config_toml = r#"theme = "theme/theme.toml"
content_dir = "content"

[theme_constants]
  title = "Rocket Documentation"

[templates]
  "*" = "default"
"#;

    let theme_toml = r#"[templates]
default = "default.html"
"#;

    let theme_html = r#"<!doctype html>
<html>
<head>
<title>{{project.title}}{{#if page.title}} - {{striptags page.title}}{{/if}}</title>
<meta charset="utf-8">
</head>
<body>
<nav class="root-toc">
{{toctree "index"}}
</nav>
<div class="body">
{{{body}}}
</div>
</body>
</html>
"#;

    let gitignore = "build/\n";

    let mut f = File::create(format!("{}/config.toml", name)).expect("Unable to create file");
    f.write_all(config_toml.as_bytes())
        .expect("Unable to write data");

    let mut f = File::create(format!("{}/theme/theme.toml", name)).expect("Unable to create file");
    f.write_all(theme_toml.as_bytes())
        .expect("Unable to write data");

    let mut f =
        File::create(format!("{}/theme/default.html", name)).expect("Unable to create file");
    f.write_all(theme_html.as_bytes())
        .expect("Unable to write data");

    let mut f = File::create(format!("{}/.gitignore", name)).expect("Unable to create file");
    f.write_all(gitignore.as_bytes())
        .expect("Unable to write data");
}
