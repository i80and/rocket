use lazycell::LazyCell;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect;

pub static DEFAULT_SYNTAX_THEME: &str = "base16-ocean.light";

pub struct SyntaxHighlighter {
    syntax_set: LazyCell<SyntaxSet>,
    theme_set: LazyCell<ThemeSet>,
    theme: String,
}

impl SyntaxHighlighter {
    pub fn new(theme: &str) -> Self {
        SyntaxHighlighter {
            syntax_set: LazyCell::new(),
            theme_set: LazyCell::new(),
            theme: theme.to_owned(),
        }
    }

    pub fn highlight(&self, language: &str, code: &str) -> Result<String, ()> {
        let syntax_set = self.syntax_set
            .borrow_with(SyntaxSet::load_defaults_newlines);
        let theme_set = self.theme_set.borrow_with(ThemeSet::load_defaults);

        let syntax = syntax_set.find_syntax_by_extension(language).ok_or(())?;
        let theme = &theme_set.themes[&self.theme];

        Ok(syntect::html::highlighted_snippet_for_string(code, syntax, theme))
    }
}
