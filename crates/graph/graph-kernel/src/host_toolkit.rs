// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

/// GUI toolkit families that should receive fresh Graphshell host adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HostToolkit {
    Iced,
    Gpui,
    HtmlCss,
    Makepad,
    Egui,
}

pub const NEW_HOST_TOOLKIT_ORDER: [HostToolkit; 5] = [
    HostToolkit::Iced,
    HostToolkit::Gpui,
    HostToolkit::HtmlCss,
    HostToolkit::Makepad,
    HostToolkit::Egui,
];

pub fn preferred_host_toolkits() -> &'static [HostToolkit] {
    &NEW_HOST_TOOLKIT_ORDER
}

impl HostToolkit {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Iced => "iced",
            Self::Gpui => "gpui",
            Self::HtmlCss => "html-css",
            Self::Makepad => "makepad",
            Self::Egui => "egui",
        }
    }

    pub const fn crate_name(self) -> &'static str {
        match self {
            Self::Iced => "graphshell-host-iced",
            Self::Gpui => "graphshell-host-gpui",
            Self::HtmlCss => "graphshell-host-html-css",
            Self::Makepad => "graphshell-host-makepad",
            Self::Egui => "graphshell-host-egui",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_host_order_keeps_egui_last() {
        assert_eq!(
            preferred_host_toolkits(),
            [
                HostToolkit::Iced,
                HostToolkit::Gpui,
                HostToolkit::HtmlCss,
                HostToolkit::Makepad,
                HostToolkit::Egui,
            ]
        );
    }

    #[test]
    fn toolkit_ids_and_crate_names_are_stable() {
        let pairs: Vec<_> = preferred_host_toolkits()
            .iter()
            .map(|toolkit| (toolkit.id(), toolkit.crate_name()))
            .collect();

        assert_eq!(
            pairs,
            [
                ("iced", "graphshell-host-iced"),
                ("gpui", "graphshell-host-gpui"),
                ("html-css", "graphshell-host-html-css"),
                ("makepad", "graphshell-host-makepad"),
                ("egui", "graphshell-host-egui"),
            ]
        );
    }
}
