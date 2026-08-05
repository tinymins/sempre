# Project UI Rules

- Commit every completed user-requested change. Keep separate requests or fixes in separate commits.
- In the React application under `ui/`, business code must use an existing control from `@acme/components` instead of rendering or styling a native replacement.
- Native control implementations belong only in `ui/src/components/acme`. If the library lacks a required control, add the smallest reusable implementation there before using it in a page or feature.
- The standalone static site under `site/` does not load the React component library and is outside this rule.
