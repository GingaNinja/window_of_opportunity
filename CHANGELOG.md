# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.1](https://github.com/GingaNinja/window_of_opportunity/compare/v0.5.0...v0.5.1) - 2026-10-08

### Fixed

- perform an accessibility test

## [0.5.0](https://github.com/GingaNinja/window_of_opportunity/compare/v0.4.3...v0.5.0) - 2026-10-08

### Fixed

- listview height is now calculated correctly for macos
- padding is top, right, bottom, left, or other combos
- add left/right padding to listview items as the selection overlay obscures the items
- items shrink, and don't just grow, if slack is negative.

## [0.4.3](https://github.com/GingaNinja/window_of_opportunity/compare/v0.4.2...v0.4.3) - 2026-10-07

### Added

- fonts size in win32

## [0.4.2](https://github.com/GingaNinja/window_of_opportunity/compare/v0.4.1...v0.4.2) - 2026-10-06

### Fixed

- handle tabstops for interactive controls

## [0.4.1](https://github.com/GingaNinja/window_of_opportunity/compare/v0.4.0...v0.4.1) - 2026-10-06

### Fixed

- update readme info

## [0.4.0](https://github.com/GingaNinja/window_of_opportunity/compare/v0.3.3...v0.4.0) - 2026-10-06

### Added

- win32 (without image)

### Fixed

- full row select for listview, removal of extra space before first item, and better handling of repaint
- fix the registry code and add placeholder for input on win32
- bubble up button clicks
- don't use handler id 0 which collides with unused handlers

### Other

- bring back the list in simples example
- measure row-height based on font size
- capture the height of the row item
- paint the item
- add one single column with the correct width
- initial listview ownerdraw
- handle row_height of the listview
- add basics of the list
- handle input for win32
- add grow(true) to simplest div to make layout options clear
- if user resized window, use new size for arranging
- add colour to win32 divs
- build also on windows
- update readme with new win32 stuff, and build in github
- handle standard WM_DESTROY for quit
- Merge branch 'win32' of github.com:GingaNinja/window_of_opportunity into win32
- update win32 to work with the libary
- move macos specific code to separate module
- separate logic of state vs platform (cacao)

## [0.3.3](https://github.com/GingaNinja/window_of_opportunity/compare/v0.3.2...v0.3.3) - 2026-09-26

### Fixed

- expand components for the list view item display

## [0.3.2](https://github.com/GingaNinja/window_of_opportunity/compare/v0.3.1...v0.3.2) - 2026-09-25

### Other

- add GUI category

## [0.3.1](https://github.com/GingaNinja/window_of_opportunity/compare/v0.3.0...v0.3.1) - 2026-09-25

### Fixed

- add custom props to custom components
- use typed props internally

### Other

- updated README and examples. Todos now adds a hard-coded todo on click

## [0.3.0](https://github.com/GingaNinja/window_of_opportunity/compare/v0.2.0...v0.3.0) - 2026-09-24

### Added

- add props (color and font_size) to the Text element
- add List element with ability to create custom views for rows

### Fixed

- add some docs for the ui! macro

### Other

- update readme with latest status and a simple example
- send data to list, and display one column
- add a hard-coded listview
- improve readme

## [0.2.0](https://github.com/GingaNinja/window_of_opportunity/compare/v0.1.2...v0.2.0) - 2026-09-17

### Added

- add a menu so we can use cmd+q/cmd+w
- set title from WindowSpec (and ui! macro)

## [0.1.2](https://github.com/GingaNinja/window_of_opportunity/compare/v0.1.1...v0.1.2) - 2026-09-17

### Fixed

- pass the 2 doc tests

### Other

- Update release-plz.yml
- add dependabot
- Create rust.yml
- Create release-plz action

## [0.1.1](https://github.com/GingaNinja/window_of_opportunity/compare/v0.1.0...v0.1.1) - 2026-09-17

### Other

- probe for debugging issues with appkit
- readme explaining features and plan
- avoid the cargo-clippy warning
- a simple example
- get a working reactive macos app
- simple reactive side
- move win32 stuff to a separate folder
- use a macro for defining UI
