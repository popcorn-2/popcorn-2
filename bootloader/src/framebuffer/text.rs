use psf::PsfFont;

pub struct FontFamily<'a> {
    regular: &'a dyn PsfFont,
    bold: Option<&'a dyn PsfFont>,
    italic: Option<&'a dyn PsfFont>,
    bold_italic: Option<&'a dyn PsfFont>
}

impl<'a> FontFamily<'a> {
    pub fn new(regular: &'a dyn PsfFont, bold: Option<&'a dyn PsfFont>, italic: Option<&'a dyn PsfFont>, bold_italic: Option<&'a dyn PsfFont>) -> Self {
        Self { regular, bold, italic, bold_italic }
    }

    pub fn get_available_style(&self, style: FontStyle) -> FontStyle {
        if self.font_exists_for_style(style) { style }
        else {
            *style.fallbacks().iter()
                .rev()
                .reduce(|current, fallback| {
                if self.font_exists_for_style(*fallback) { fallback }
                else { current }
            }).unwrap()
        }
    }

    fn font_exists_for_style(&self, style: FontStyle) -> bool {
        match style {
            FontStyle::Regular => true,
            FontStyle::Bold => self.bold.is_some(),
            FontStyle::Italic => self.italic.is_some(),
            FontStyle::BoldItalic => self.bold_italic.is_some()
        }
    }

    fn get_font_for_style(&self, style: FontStyle) -> &dyn PsfFont {
        match self.get_available_style(style) {
            FontStyle::Regular => self.regular,
            FontStyle::Bold => self.bold.unwrap(),
            FontStyle::Italic => self.italic.unwrap(),
            FontStyle::BoldItalic => self.bold_italic.unwrap()
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum FontStyle {
    Regular,
    Bold,
    Italic,
    BoldItalic
}

impl FontStyle {
    pub fn fallbacks(&self) -> &'static [Self] {
        match *self {
            Self::Regular => &[Self::Regular],
            Self::Bold => &[Self::Regular],
            Self::Italic => &[Self::Regular],
            Self::BoldItalic => &[Self::Bold, Self::Italic, Self::Regular]
        }
    }
}
