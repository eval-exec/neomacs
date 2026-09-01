# Neomacs tool-bar icon themes

This directory contains SVG assets used by `neomacs-toolbar-icon-theme`.
The resolver maps GNU tool-bar image base names such as `save.xpm` to
theme files such as `material/save.svg`.  Missing theme files intentionally
fall back to the `neomacs` theme and then to GNU's original image lookup.

Sources and licenses:

- `neomacs`: Neomacs SVG toolbar icons from `crates/neomacs-display-runtime/icons/toolbar`.
- `vscode-like`: Microsoft VS Code Codicons SVGs from <https://github.com/microsoft/vscode-codicons>, licensed under CC-BY-4.0.
- `jetbrains-like`: JetBrains IntelliJ Platform SVGs from <https://github.com/JetBrains/intellij-community/tree/master/platform/icons/src/actions>, licensed under Apache-2.0.
- `atom-like`: GitHub Primer Octicons SVGs from <https://github.com/primer/octicons>, licensed under MIT.
- `material`: Google Material Design Icons SVGs from <https://github.com/google/material-design-icons>, licensed under Apache-2.0.

Vendored upstream icons are not redrawn; they are only renamed into GNU
tool-bar image base names such as `open.svg`, `search-replace.svg`, and
`mail/compose.svg`.

These theme names describe visual style families.  They are not product
logos, and the assets are stored as ordinary toolbar action icons.
