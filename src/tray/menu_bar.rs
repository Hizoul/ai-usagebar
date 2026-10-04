//! Chart, logo or short-name strip for the macOS status item.
//!
//! `StripContent` owns visible groups and their values; the report supplies
//! the short name drawn in place of a mark, either because the provider has
//! no embedded mark or because the user chose the name look.

use serde_json::Value;

use super::strip::{StripContent, StripMetric, default_group};

/// Artwork needed for the current status-item content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StatusItemContent {
    /// The static application icon is the empty-content fallback.
    AppIcon,
    /// Draw the usage bars.
    Chart,
    /// Draw provider logos and starred metric values.
    Logos,
}

/// The menu-bar look chosen in Settings → Menu Bar → Menu Bar Shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MenuBarLook {
    /// The usage bars (default).
    Chart,
    /// Each provider's logo followed by its starred values.
    Logos,
    /// Each provider's short name (`cld`, `cdx`, …) followed by its starred
    /// values, the way the Quattro and Waybar bars tag a provider.
    Name,
}

impl MenuBarLook {
    /// Read `[tray] menu_bar_style`. Anything unrecognised keeps the chart, so
    /// a config written by a newer build never blanks the status item.
    pub(super) fn from_style(style: Option<&str>) -> Self {
        match style {
            Some("provider") => Self::Logos,
            Some("name") => Self::Name,
            _ => Self::Chart,
        }
    }

    /// The `[tray] menu_bar_style` value persisted for this look.
    pub(super) fn style(self) -> &'static str {
        match self {
            Self::Chart => "bars",
            Self::Logos => "provider",
            Self::Name => "name",
        }
    }

    /// The value the popover's picker uses for this look.
    pub(super) fn picker_value(self) -> &'static str {
        match self {
            Self::Chart => "chart",
            Self::Logos => "logos",
            Self::Name => "name",
        }
    }

    /// Parse the popover's picker value.
    pub(super) fn from_picker_value(value: &str) -> Option<Self> {
        match value {
            "chart" => Some(Self::Chart),
            "logos" => Some(Self::Logos),
            "name" => Some(Self::Name),
            _ => None,
        }
    }
}

/// Select status-item artwork, falling back to the app icon when content is empty.
pub(super) fn status_item_content(look: MenuBarLook, has_content: bool) -> StatusItemContent {
    if !has_content {
        StatusItemContent::AppIcon
    } else if look == MenuBarLook::Chart {
        StatusItemContent::Chart
    } else {
        StatusItemContent::Logos
    }
}

/// One entry of the emergency menu attached to the status item when the
/// popover's WKWebView could not be built (#249).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FallbackItem {
    /// muda menu id the host matches `MenuEvent`s against.
    pub(super) id: &'static str,
    pub(super) label: &'static str,
}

/// The fallback status-item menu for the webview-less tray (#249): an
/// accessory app (no Dock icon, no app menu) otherwise has no quit affordance
/// short of `killall`. Refresh stays because the status-item readout works
/// without the webview; Quit terminates through the same loop exit the
/// popover's own Quit control uses.
pub(super) fn fallback_menu_items() -> [FallbackItem; 2] {
    [
        FallbackItem {
            id: "fallback-refresh",
            label: "Refresh",
        },
        FallbackItem {
            id: "fallback-quit",
            label: "Quit AI Usage",
        },
    ]
}

/// The fallback menu exists **only** while the popover's webview is absent.
/// Normal operation keeps the status item menu-free (see `build_tray`) so
/// both mouse buttons reach the popover; attaching a menu makes AppKit
/// intercept clicks, which is exactly the trade wanted when there is no
/// popover to open.
pub(super) fn fallback_menu_attached(webview_built: bool) -> bool {
    !webview_built
}

/// One provider's visible logo (or short name) and starred values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LogoSegment {
    /// Lowercase provider slug used to find its embedded mark.
    pub(super) slug: String,
    /// Report short name, drawn when the mark cannot be drawn, or beside it
    /// when `with_name` is set.
    pub(super) short_name: Option<String>,
    /// The name look: draw `short_name` after the mark rather than only
    /// where the mark is missing.
    pub(super) with_name: bool,
    /// Non-empty metric values in star order; their count is the rendered line count.
    pub(super) values: Vec<String>,
}

/// The id of the one provider the name look draws: the popover's selected
/// provider, else the report's `primary`, else the first group with a value.
/// `None` only when no group has a value to show.
fn focused_group_id<'a>(
    groups: &'a [(String, String, Vec<StripMetric>)],
    selected: Option<&str>,
    report: &Value,
) -> Option<&'a str> {
    let shown: Vec<&str> = groups
        .iter()
        .filter(|(_, _, metrics)| metrics.iter().any(|metric| !metric.value.trim().is_empty()))
        .map(|(id, _, _)| id.as_str())
        .collect();
    let primary = report.get("primary").and_then(Value::as_str);
    [selected, primary]
        .into_iter()
        .flatten()
        .find_map(|wanted| shown.iter().copied().find(|id| *id == wanted))
        .or_else(|| shown.first().copied())
}

/// Build logo segments from the same starred metric groups used by the chart.
/// The name look keeps one provider, `selected` or its fallbacks, and one
/// value, like the Quattro bar's `icon SHORT 54%` chip, with the short name
/// beside the mark unless `show_short_name` is off; the logos look keeps
/// every group with up to two stacked values.
pub(super) fn logo_segments(
    content: &StripContent,
    report: &Value,
    look: MenuBarLook,
    selected: Option<&str>,
    show_short_name: bool,
) -> Vec<LogoSegment> {
    if look == MenuBarLook::Name {
        return name_segment(content, report, selected, show_short_name)
            .into_iter()
            .collect();
    }
    content
        .groups
        .iter()
        .filter_map(|(id, _, metrics)| segment(id, metrics, report, 2, false))
        .collect()
}

/// The name look's one chip. A selected provider with no starred metric
/// still wins over the fallbacks, drawn with its default metric, because
/// the user picked it in the popover.
fn name_segment(
    content: &StripContent,
    report: &Value,
    selected: Option<&str>,
    show_short_name: bool,
) -> Option<LogoSegment> {
    let starred = |id: &str| content.groups.iter().any(|(group, _, _)| group == id);
    if let Some((id, _, metrics)) = selected
        .filter(|id| !starred(id))
        .and_then(|id| default_group(report, id))
    {
        return name_chip(&id, &metrics, report, show_short_name);
    }
    let focus = focused_group_id(&content.groups, selected, report)?;
    let (id, _, metrics) = content.groups.iter().find(|(id, _, _)| id == focus)?;
    name_chip(id, metrics, report, show_short_name)
}

/// The chip shows the provider's highest-percent metric, like the Quattro
/// bar's default `auto` window (`omarchy/Model.js` `maxPercent`): a spent
/// weekly window must not hide behind an idle 5h session reading 0%.
/// Without `show_short_name` the mark stands alone, like the logos look,
/// which still falls back to the name for a provider with no mark.
fn name_chip(
    id: &str,
    metrics: &[StripMetric],
    report: &Value,
    show_short_name: bool,
) -> Option<LogoSegment> {
    let highest = highest_metric(metrics)?;
    segment(
        id,
        std::slice::from_ref(highest),
        report,
        1,
        show_short_name,
    )
}

/// The bounded metric with the largest used fraction, the first one on a
/// tie; the first metric with a value when none is bounded.
fn highest_metric(metrics: &[StripMetric]) -> Option<&StripMetric> {
    let shown = || metrics.iter().filter(|m| !m.value.trim().is_empty());
    let highest_bounded =
        shown()
            .filter(|m| m.bounded)
            .fold(None, |best: Option<&StripMetric>, m| match best {
                Some(best) if best.fraction >= m.fraction => Some(best),
                _ => Some(m),
            });
    highest_bounded.or_else(|| shown().next())
}

/// One provider's segment: up to `value_cap` non-empty values, or `None`
/// when it has no value or no usable slug.
fn segment(
    id: &str,
    metrics: &[StripMetric],
    report: &Value,
    value_cap: usize,
    with_name: bool,
) -> Option<LogoSegment> {
    let values: Vec<String> = metrics
        .iter()
        .map(|metric| metric.value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .take(value_cap)
        .collect();
    if values.is_empty() {
        return None;
    }
    let slug = id.split('@').next()?.to_ascii_lowercase();
    if slug.is_empty() {
        return None;
    }
    Some(LogoSegment {
        slug,
        short_name: short_name_for(report, id),
        with_name,
        values,
    })
}

/// Build tooltip lines for the starred groups, or the app name when none have values.
pub(super) fn tooltip(content: &StripContent) -> String {
    let lines: Vec<String> = content
        .groups
        .iter()
        .filter_map(|(_, name, metrics)| {
            let values: Vec<&str> = metrics
                .iter()
                .map(|metric| metric.value.trim())
                .filter(|value| !value.is_empty())
                .collect();
            if values.is_empty() {
                return None;
            }
            Some(format!("{} · {}", safe_text(name), values.join(" ")))
        })
        .collect();
    if lines.is_empty() {
        "AI Usage".into()
    } else {
        lines.join("\n")
    }
}

fn short_name_for(report: &Value, id: &str) -> Option<String> {
    report
        .get("entries")?
        .as_array()?
        .iter()
        .find(|entry| entry.get("id").and_then(Value::as_str) == Some(id))?
        .get("short_name")?
        .as_str()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(safe_text)
}

fn safe_text(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_modes_fall_back_to_static_app_icon() {
        for look in [MenuBarLook::Chart, MenuBarLook::Logos, MenuBarLook::Name] {
            assert_eq!(
                status_item_content(look, false),
                StatusItemContent::AppIcon,
                "{look:?}"
            );
        }
        assert_eq!(
            status_item_content(MenuBarLook::Chart, true),
            StatusItemContent::Chart
        );
        assert_eq!(
            status_item_content(MenuBarLook::Logos, true),
            StatusItemContent::Logos
        );
        // Name reuses the strip image: only the label differs from Logos.
        assert_eq!(
            status_item_content(MenuBarLook::Name, true),
            StatusItemContent::Logos
        );
    }

    #[test]
    fn menu_bar_look_round_trips_through_config_and_picker() {
        for look in [MenuBarLook::Chart, MenuBarLook::Logos, MenuBarLook::Name] {
            assert_eq!(MenuBarLook::from_style(Some(look.style())), look);
            assert_eq!(
                MenuBarLook::from_picker_value(look.picker_value()),
                Some(look)
            );
        }
    }

    /// An unset, unknown or future `menu_bar_style` keeps the chart default.
    #[test]
    fn menu_bar_look_defaults_to_the_chart() {
        assert_eq!(MenuBarLook::from_style(None), MenuBarLook::Chart);
        assert_eq!(MenuBarLook::from_style(Some("bars")), MenuBarLook::Chart);
        assert_eq!(
            MenuBarLook::from_style(Some("sparkles")),
            MenuBarLook::Chart
        );
        assert_eq!(MenuBarLook::from_picker_value("sparkles"), None);
    }

    /// #249: the emergency menu attaches only when the webview is absent, so
    /// normal operation — menu-free status item, clicks open the popover — is
    /// untouched.
    #[test]
    fn fallback_menu_attaches_only_without_the_webview() {
        assert!(!fallback_menu_attached(true));
        assert!(fallback_menu_attached(false));
    }

    /// The host matches `MenuEvent` ids against these strings, so they must be
    /// unique and Quit must be the terminal action of the menu.
    #[test]
    fn fallback_menu_items_have_unique_nonempty_ids_ending_in_quit() {
        let items = fallback_menu_items();
        assert!(items.iter().all(|item| !item.id.is_empty()));
        assert!(items.iter().all(|item| !item.label.is_empty()));
        let ids: Vec<&str> = items.iter().map(|item| item.id).collect();
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(ids.len(), unique.len(), "ids must be unique: {ids:?}");
        assert_eq!(items[items.len() - 1].id, "fallback-quit");
        assert_eq!(items[0].id, "fallback-refresh");
    }

    #[test]
    fn logo_segments_keep_one_star_value_and_use_star_order() {
        let content = StripContent {
            groups: vec![
                ("anthropic".into(), "Claude".into(), vec![metric("41%")]),
                (
                    "openai".into(),
                    "Codex".into(),
                    vec![metric("9%"), metric("5%")],
                ),
            ],
            bars: Vec::new(),
        };
        let report = json!({"entries":[]});

        let segments = logo_segments(&content, &report, MenuBarLook::Logos, None, true);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].slug, "anthropic");
        assert_eq!(segments[0].values, vec![String::from("41%")]);
        assert_eq!(segments[1].slug, "openai");
        assert_eq!(
            segments[1].values,
            vec![String::from("9%"), String::from("5%")]
        );
    }

    #[test]
    fn logo_segments_skip_groups_without_values() {
        let content = StripContent {
            groups: vec![
                ("cursor".into(), "Cursor".into(), vec![metric("  ")]),
                ("zai".into(), "Z.AI".into(), vec![metric("12%")]),
            ],
            bars: Vec::new(),
        };

        let segments = logo_segments(
            &content,
            &json!({"entries":[]}),
            MenuBarLook::Logos,
            None,
            true,
        );
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].slug, "zai");
        assert_eq!(segments[0].values, vec![String::from("12%")]);
    }

    #[test]
    fn content_without_metric_values_stays_empty_in_both_modes() {
        let report = json!({"entries":[
            {"id":"claude", "display_name":"Claude", "sections":[
                {"type":"metric", "label":"Weekly"}
            ]}
        ]});
        let content = super::super::strip::content_from_payload(
            &report,
            &super::super::strip::Stars::new(),
            &[],
        );

        assert!(content.groups.is_empty());
        assert!(content.bars.is_empty());
        assert!(logo_segments(&content, &report, MenuBarLook::Logos, None, true).is_empty());
    }

    #[test]
    fn logo_segments_use_report_short_name_when_mark_is_unknown() {
        let content = StripContent {
            groups: vec![(
                "unknown@work".into(),
                "Unknown Provider".into(),
                vec![metric("8%")],
            )],
            bars: Vec::new(),
        };
        let report = json!({"entries":[
            {"id":"unknown@work", "short_name":"unk"}
        ]});

        let segments = logo_segments(&content, &report, MenuBarLook::Logos, None, true);
        assert_eq!(segments[0].slug, "unknown");
        assert!(super::super::marks::mark_svg(&segments[0].slug).is_none());
        assert_eq!(segments[0].short_name.as_deref(), Some("unk"));
    }

    /// The name look keeps the mark and adds the short name beside it; the
    /// logos look keeps the mark alone.
    #[test]
    fn name_look_adds_the_short_name_beside_the_mark() {
        let content = StripContent {
            groups: vec![("anthropic".into(), "Claude".into(), vec![metric("41%")])],
            bars: Vec::new(),
        };
        let report = json!({"entries":[{"id":"anthropic", "short_name":"cld"}]});

        let chip = logo_segments(&content, &report, MenuBarLook::Name, None, true);
        assert!(chip[0].with_name);
        assert_eq!(chip[0].short_name.as_deref(), Some("cld"));
        assert!(super::super::marks::mark_svg(&chip[0].slug).is_some());

        let logos = logo_segments(&content, &report, MenuBarLook::Logos, None, true);
        assert!(!logos[0].with_name);
    }

    /// With the short name turned off the chip is the mark and the value; a
    /// provider with no mark still gets its name, since nothing else would
    /// tell which provider the value belongs to.
    #[test]
    fn name_look_can_leave_out_the_short_name() {
        let content = StripContent {
            groups: vec![
                ("kimi".into(), "Kimi".into(), vec![metric("42%")]),
                ("unknown".into(), "Unknown".into(), vec![metric("8%")]),
            ],
            bars: Vec::new(),
        };
        let report = json!({"primary":null,"entries":[
            {"id":"kimi", "short_name":"kmi"},
            {"id":"unknown", "short_name":"unk"}
        ]});

        let chip = logo_segments(&content, &report, MenuBarLook::Name, Some("kimi"), false);
        assert!(!chip[0].with_name);
        assert!(super::super::marks::mark_svg(&chip[0].slug).is_some());
        assert_eq!(chip[0].values, vec![String::from("42%")]);

        let shown = logo_segments(&content, &report, MenuBarLook::Name, Some("kimi"), true);
        assert!(shown[0].with_name);

        let unmarked = logo_segments(&content, &report, MenuBarLook::Name, Some("unknown"), false);
        assert!(super::super::marks::mark_svg(&unmarked[0].slug).is_none());
        assert_eq!(unmarked[0].short_name.as_deref(), Some("unk"));
    }

    fn two_groups() -> StripContent {
        StripContent {
            groups: vec![
                (
                    "anthropic".into(),
                    "Claude".into(),
                    vec![metric("54%"), metric("16%")],
                ),
                ("openai".into(), "Codex".into(), vec![metric("100%")]),
            ],
            bars: Vec::new(),
        }
    }

    /// Quattro draws one chip for the selected provider: one provider and one
    /// value, where the logos look stacks two values for every provider.
    #[test]
    fn name_look_shows_only_the_selected_provider_and_its_first_value() {
        let report = json!({"primary":"anthropic","entries":[]});

        let chip = logo_segments(
            &two_groups(),
            &report,
            MenuBarLook::Name,
            Some("openai"),
            true,
        );
        assert_eq!(chip.len(), 1);
        assert_eq!(chip[0].slug, "openai");
        assert_eq!(chip[0].values, vec![String::from("100%")]);

        let claude = logo_segments(
            &two_groups(),
            &report,
            MenuBarLook::Name,
            Some("anthropic"),
            true,
        );
        assert_eq!(claude[0].values, vec![String::from("54%")]);

        let logos = logo_segments(
            &two_groups(),
            &report,
            MenuBarLook::Logos,
            Some("openai"),
            true,
        );
        assert_eq!(logos.len(), 2);
        assert_eq!(logos[0].values.len(), 2);
    }

    /// Before the popover reports a selection the report's `primary` stands
    /// in, then the first provider with a value; a stale selection (the
    /// provider was unstarred or disabled) falls through the same way.
    #[test]
    fn name_look_falls_back_from_selection_to_primary_to_first() {
        let with_primary = json!({"primary":"openai","entries":[]});
        let no_primary = json!({"primary":null,"entries":[]});
        let slug = |report: &Value, selected: Option<&str>| {
            logo_segments(&two_groups(), report, MenuBarLook::Name, selected, true)[0]
                .slug
                .clone()
        };

        assert_eq!(slug(&with_primary, None), "openai");
        assert_eq!(slug(&with_primary, Some("gone")), "openai");
        assert_eq!(slug(&no_primary, None), "anthropic");
        assert_eq!(slug(&no_primary, Some("gone")), "anthropic");
    }

    /// Selecting a provider with no starred metric used to fall through to
    /// the primary: SuperGrok selected drew Z.AI's chip. The selection now
    /// draws its own first bounded metric from the report.
    #[test]
    fn name_look_shows_a_selected_provider_that_has_no_star() {
        let content = StripContent {
            groups: vec![("zai".into(), "Z.AI".into(), vec![metric("0%")])],
            bars: Vec::new(),
        };
        let report = json!({"primary":"zai","entries":[
            {"id":"zai","short_name":"zai","status":"ready","sections":[
                {"type":"metric","label":"Session","percent":0,"value":"0%"}]},
            {"id":"supergrok","short_name":"sgk","status":"ready","sections":[
                {"type":"metric","label":"Weekly usage","percent":7,"value":"7%"}]},
        ]});

        let chip = logo_segments(
            &content,
            &report,
            MenuBarLook::Name,
            Some("supergrok"),
            true,
        );
        assert_eq!(chip.len(), 1);
        assert_eq!(chip[0].slug, "supergrok");
        assert_eq!(chip[0].short_name.as_deref(), Some("sgk"));
        assert_eq!(chip[0].values, vec![String::from("7%")]);

        let unselected = logo_segments(&content, &report, MenuBarLook::Name, None, true);
        assert_eq!(unselected[0].slug, "zai");
    }

    /// A provider whose starred values are all empty is not a candidate, so
    /// the name look never draws a bare name with no number.
    #[test]
    fn name_look_skips_a_selected_provider_without_a_value() {
        let content = StripContent {
            groups: vec![
                ("cursor".into(), "Cursor".into(), vec![metric("  ")]),
                ("zai".into(), "Z.AI".into(), vec![metric("12%")]),
            ],
            bars: Vec::new(),
        };
        let report = json!({"primary":null,"entries":[]});

        let segments = logo_segments(&content, &report, MenuBarLook::Name, Some("cursor"), true);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].slug, "zai");
    }

    #[test]
    fn tooltip_lists_each_starred_group_and_uses_generic_empty_text() {
        let content = StripContent {
            groups: vec![
                (
                    "anthropic".into(),
                    "Claude".into(),
                    vec![metric("41%"), metric("5%")],
                ),
                ("openai".into(), "Codex".into(), vec![metric("9%")]),
                ("cursor".into(), "Cursor".into(), vec![metric("")]),
            ],
            bars: Vec::new(),
        };

        assert_eq!(tooltip(&content), "Claude · 41% 5%\nCodex · 9%");
        assert_eq!(
            tooltip(&StripContent {
                groups: Vec::new(),
                bars: Vec::new(),
            }),
            "AI Usage"
        );
    }

    fn metric(value: &str) -> super::super::strip::StripMetric {
        super::super::strip::StripMetric {
            provider_id: String::new(),
            provider_name: String::new(),
            key: String::new(),
            label: String::new(),
            value: value.to_owned(),
            fraction: 0.0,
            bounded: true,
        }
    }

    fn used(value: &str, fraction: f64) -> super::super::strip::StripMetric {
        super::super::strip::StripMetric {
            fraction,
            ..metric(value)
        }
    }

    /// Z.AI with the weekly window spent and the 5h session idle drew
    /// `zai 0%`. Like Quattro's `auto` window the chip shows the highest
    /// percent, so it reads `zai 100%`; an unbounded value never outranks one.
    #[test]
    fn name_look_shows_the_highest_percent_metric() {
        let report = json!({"primary":null,"entries":[]});
        let spent_weekly = StripContent {
            groups: vec![(
                "zai".into(),
                "Z.AI".into(),
                vec![used("0%", 0.0), used("100%", 1.0)],
            )],
            bars: Vec::new(),
        };
        let chip = logo_segments(&spent_weekly, &report, MenuBarLook::Name, Some("zai"), true);
        assert_eq!(chip[0].values, vec![String::from("100%")]);

        let balance = super::super::strip::StripMetric {
            bounded: false,
            ..used("$40", 0.0)
        };
        let mixed = StripContent {
            groups: vec![(
                "openrouter".into(),
                "OpenRouter".into(),
                vec![balance, used("12%", 0.12)],
            )],
            bars: Vec::new(),
        };
        let chip = logo_segments(&mixed, &report, MenuBarLook::Name, None, true);
        assert_eq!(chip[0].values, vec![String::from("12%")]);

        let logos = logo_segments(&spent_weekly, &report, MenuBarLook::Logos, None, true);
        assert_eq!(
            logos[0].values,
            vec![String::from("0%"), String::from("100%")]
        );
    }
}
