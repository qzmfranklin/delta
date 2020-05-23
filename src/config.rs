use std::process;
use std::str::FromStr;

use ansi_term::Color;
use syntect::highlighting::Color as SyntectColor;
use syntect::highlighting::Style as SyntectStyle;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::bat::output::PagingMode;
use crate::bat::terminal::to_ansi_color;
use crate::cli;
use crate::env;
use crate::style::{self, Style};
use crate::syntect_color;

pub struct Config<'a> {
    pub theme: Option<Theme>,
    pub theme_name: String,
    pub dummy_theme: Theme,
    pub max_line_distance: f64,
    pub max_line_distance_for_naively_paired_lines: f64,
    pub minus_style: Style,
    pub minus_emph_style: Style,
    pub minus_non_emph_style: Style,
    pub zero_style: Style,
    pub plus_style: Style,
    pub plus_emph_style: Style,
    pub plus_non_emph_style: Style,
    pub minus_line_marker: &'a str,
    pub plus_line_marker: &'a str,
    pub commit_style: cli::SectionStyle,
    pub commit_color: Color,
    pub file_style: cli::SectionStyle,
    pub file_color: Color,
    pub hunk_style: cli::SectionStyle,
    pub hunk_color: Color,
    pub syntax_set: SyntaxSet,
    pub terminal_width: usize,
    pub true_color: bool,
    pub background_color_extends_to_terminal_width: bool,
    pub tab_width: usize,
    pub null_style: Style,
    pub null_syntect_style: SyntectStyle,
    pub max_buffered_lines: usize,
    pub paging_mode: PagingMode,
}

pub fn get_config<'a>(
    opt: cli::Opt,
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    true_color: bool,
    terminal_width: usize,
    paging_mode: PagingMode,
) -> Config<'a> {
    let background_color_extends_to_terminal_width = opt.width != Some("variable".to_string());
    let theme_name_from_bat_pager = env::get_env_var("BAT_THEME");
    let (is_light_mode, theme_name) = get_is_light_mode_and_theme_name(
        opt.theme.as_ref(),
        theme_name_from_bat_pager.as_ref(),
        opt.light,
        &theme_set,
    );

    let (
        minus_style,
        minus_emph_style,
        minus_non_emph_style,
        zero_style,
        plus_style,
        plus_emph_style,
        plus_non_emph_style,
    ) = make_hunk_styles(&opt, is_light_mode, true_color);

    let theme = if style::is_no_syntax_highlighting_theme_name(&theme_name) {
        None
    } else {
        Some(theme_set.themes[&theme_name].clone())
    };
    let dummy_theme = theme_set.themes.values().next().unwrap().clone();

    let minus_line_marker = if opt.keep_plus_minus_markers {
        "-"
    } else {
        " "
    };
    let plus_line_marker = if opt.keep_plus_minus_markers {
        "+"
    } else {
        " "
    };

    let max_line_distance_for_naively_paired_lines =
        env::get_env_var("DELTA_EXPERIMENTAL_MAX_LINE_DISTANCE_FOR_NAIVELY_PAIRED_LINES")
            .map(|s| s.parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0);

    Config {
        theme,
        theme_name,
        dummy_theme,
        max_line_distance: opt.max_line_distance,
        max_line_distance_for_naively_paired_lines,
        minus_style,
        minus_emph_style,
        minus_non_emph_style,
        zero_style,
        plus_style,
        plus_emph_style,
        plus_non_emph_style,
        minus_line_marker,
        plus_line_marker,
        commit_style: opt.commit_style,
        commit_color: color_from_rgb_or_ansi_code(&opt.commit_color, true_color),
        file_style: opt.file_style,
        file_color: color_from_rgb_or_ansi_code(&opt.file_color, true_color),
        hunk_style: opt.hunk_style,
        hunk_color: color_from_rgb_or_ansi_code(&opt.hunk_color, true_color),
        true_color,
        terminal_width,
        background_color_extends_to_terminal_width,
        tab_width: opt.tab_width,
        syntax_set,
        null_style: Style::new(),
        null_syntect_style: SyntectStyle::default(),
        max_buffered_lines: 32,
        paging_mode,
    }
}

/// Return a (theme_name, is_light_mode) tuple.
/// theme_name == None in return value means syntax highlighting is disabled.
///
/// There are two types of color choices that have to be made:
/// 1. The choice of "theme". This is the language syntax highlighting theme; you have to make this choice when using `bat` also.
/// 2. The choice of "light vs dark mode". This determines whether the background colors should be chosen for a light or dark terminal background. (`bat` has no equivalent.)
///
/// Basically:
/// 1. The theme is specified by the `--theme` option. If this isn't supplied then it is specified by the `BAT_PAGER` environment variable.
/// 2. Light vs dark mode is specified by the `--light` or `--dark` options. If these aren't supplied then it is inferred from the chosen theme.
///
/// In the absence of other factors, the default assumes a dark terminal background.
///
/// Specifically, the rules are as follows:
///
/// | --theme    | $BAT_THEME | --light/--dark | Behavior                                                                   |
/// |------------|------------|----------------|----------------------------------------------------------------------------|
/// | -          | -          | -              | default dark theme, dark mode                                              |
/// | some_theme | (IGNORED)  | -              | some_theme with light/dark mode inferred accordingly                       |
/// | -          | BAT_THEME  | -              | BAT_THEME, with light/dark mode inferred accordingly                       |
/// | -          | -          | yes            | default light/dark theme, light/dark mode                                  |
/// | some_theme | (IGNORED)  | yes            | some_theme, light/dark mode (even if some_theme conflicts with light/dark) |
/// | -          | BAT_THEME  | yes            | BAT_THEME, light/dark mode (even if BAT_THEME conflicts with light/dark)   |
fn get_is_light_mode_and_theme_name(
    theme_arg: Option<&String>,
    bat_theme_env_var: Option<&String>,
    light_mode_arg: bool,
    theme_set: &ThemeSet,
) -> (bool, String) {
    let theme_arg = valid_theme_name_or_none(theme_arg, theme_set);
    let bat_theme_env_var = valid_theme_name_or_none(bat_theme_env_var, theme_set);
    match (theme_arg, bat_theme_env_var, light_mode_arg) {
        (None, None, false) => (false, style::DEFAULT_DARK_THEME.to_string()),
        (Some(theme_name), _, false) => (style::is_light_theme(&theme_name), theme_name),
        (None, Some(theme_name), false) => (style::is_light_theme(&theme_name), theme_name),
        (None, None, true) => (true, style::DEFAULT_LIGHT_THEME.to_string()),
        (Some(theme_name), _, is_light_mode) => (is_light_mode, theme_name),
        (None, Some(theme_name), is_light_mode) => (is_light_mode, theme_name),
    }
}

// At this stage the theme name is considered valid if it is either a real theme name or the special
// no-syntax-highlighting name.
fn valid_theme_name_or_none(theme_name: Option<&String>, theme_set: &ThemeSet) -> Option<String> {
    match theme_name {
        Some(name)
            if style::is_no_syntax_highlighting_theme_name(name)
                || theme_set.themes.contains_key(name) =>
        {
            Some(name.to_string())
        }
        _ => None,
    }
}

fn make_hunk_styles<'a>(
    opt: &'a cli::Opt,
    is_light_mode: bool,
    true_color: bool,
) -> (Style, Style, Style, Style, Style, Style, Style) {
    let minus_style = parse_style_string(
        &opt.minus_style,
        None,
        Some(style::get_minus_background_color_default(
            is_light_mode,
            true_color,
        )),
        true_color,
    );

    let minus_emph_style = parse_style_string(
        &opt.minus_emph_style,
        None,
        Some(style::get_minus_emph_background_color_default(
            is_light_mode,
            true_color,
        )),
        true_color,
    );

    // The non-emph styles default to the base style.
    let minus_non_emph_style = match &opt.minus_non_emph_style {
        Some(style_string) => parse_style_string(
            &style_string,
            None,
            minus_style.ansi_term_style.background,
            true_color,
        ),
        None => minus_style,
    };

    let zero_style = parse_style_string(&opt.zero_style, None, None, true_color);

    let plus_style = parse_style_string(
        &opt.plus_style,
        None,
        Some(style::get_plus_background_color_default(
            is_light_mode,
            true_color,
        )),
        true_color,
    );

    let plus_emph_style = parse_style_string(
        &opt.plus_emph_style,
        None,
        Some(style::get_plus_emph_background_color_default(
            is_light_mode,
            true_color,
        )),
        true_color,
    );

    // The non-emph styles default to the base style.
    let plus_non_emph_style = match &opt.plus_non_emph_style {
        Some(style_string) => parse_style_string(
            &style_string,
            None,
            plus_style.ansi_term_style.background,
            true_color,
        ),
        None => plus_style,
    };

    (
        minus_style,
        minus_emph_style,
        minus_non_emph_style,
        zero_style,
        plus_style,
        plus_emph_style,
        plus_non_emph_style,
    )
}

/// Construct ansi_term Style from style string supplied on command line,
/// together with defaults.
pub fn parse_style_string(
    style_string: &str,
    foreground_default: Option<Color>,
    background_default: Option<Color>,
    true_color: bool,
) -> Style {
    let mut style = Style::new();
    let mut seen_foreground = false;
    let mut seen_background = false;
    for s in style_string.to_lowercase().split_whitespace() {
        if s == "blink" {
            style.ansi_term_style.is_blink = true;
        } else if s == "bold" {
            style.ansi_term_style.is_bold = true;
        } else if s == "dimmed" {
            style.ansi_term_style.is_dimmed = true;
        } else if s == "hidden" {
            style.ansi_term_style.is_hidden = true;
        } else if s == "italic" {
            style.ansi_term_style.is_italic = true;
        } else if s == "reverse" {
            style.ansi_term_style.is_reverse = true;
        } else if s == "strikethrough" {
            style.ansi_term_style.is_strikethrough = true;
        } else if s == "underline" {
            style.ansi_term_style.is_underline = true;
        } else if !seen_foreground {
            if s == "syntax" {
                style.is_syntax_highlighted = true;
            } else {
                style.ansi_term_style.foreground =
                    color_from_rgb_or_ansi_code_with_default(s, foreground_default, true_color);
            }
            seen_foreground = true;
        } else if !seen_background {
            style.ansi_term_style.background =
                color_from_rgb_or_ansi_code_with_default(s, background_default, true_color);
            seen_background = true;
        } else {
            eprintln!(
                "Invalid style string: {}.\n\
                 A style string may contain a foreground color string. \
                 If it contains a foreground color string it may subsequently \
                 contain a background color string. Font style attributes \
                 'bold', 'italic', and 'underline' may occur in any position. \
                 All strings must be separated by spaces. \
                 See delta --help for how to specify colors.",
                s
            );
            process::exit(1);
        }
    }
    style
}

pub fn color_from_rgb_or_ansi_code(s: &str, true_color: bool) -> Color {
    let die = || {
        eprintln!("Invalid color: {}", s);
        process::exit(1);
    };
    let syntect_color = if s.starts_with("#") {
        SyntectColor::from_str(s).unwrap_or_else(|_| die())
    } else {
        s.parse::<u8>()
            .ok()
            .and_then(syntect_color::syntect_color_from_ansi_number)
            .or_else(|| syntect_color::syntect_color_from_ansi_name(s))
            .unwrap_or_else(die)
    };
    to_ansi_color(syntect_color, true_color)
}

fn color_from_rgb_or_ansi_code_with_default(
    arg: &str,
    default: Option<Color>,
    true_color: bool,
) -> Option<Color> {
    let arg = arg.to_lowercase();
    if arg == "normal" {
        None
    } else if arg == "auto" {
        default
    } else {
        Some(color_from_rgb_or_ansi_code(&arg, true_color))
    }
}
