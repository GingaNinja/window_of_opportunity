# Window_of_opportunity

A massively under-developed WIP, aiming to implement a react style library for creating native (as in, using the actual OS primitives) GUIs. 

Currently targetting Macos with AppKit (using Cacao under the hood), but wanting to add some win32 waiting in the sidelines.

## Features

* Input boxes support internationlization because they are the native input boxes.
* Small binaries - currently the library is less than 2k lines, plus a dependency on Cacao.

## Getting started

Checkout the examples folder, there's at least one example in there which is doing not very much, but shows click event handlers.

## Project Status
Note, hardly anything works, but creating a basic application is possible, and there are hooks for some state.

No roadmap, but some things I want to do:
* Set title bar text (currently it's hard-coded). Should be resettable on re-renders
* Handle vec based lists
* Add more elements - scrollviews, radiobuttons, comboboxes, selectboxes.
* Add more properties - border, rounded corners, other events
* Get working with win32
* Get working with gtk.
* Add a test target for writing automated tests against the virtual dom