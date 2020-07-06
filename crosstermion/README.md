
Crosstermion is a utility crate to unify some types of both crates, allowing to easily build apps that use the leaner `termion` 
crate on unix systems, but resort to crossterm on windows systems.

Currently provided facilities are:

* a `Key` type an an `input_stream` (_async_) to receive key presses
* an `AltenrativeRawTerminal` which marries an alternative screen with raw mode
* a way to create a `tui` or `tui-react` terminal with either the crossterm or the termion backend.

### Features

All features work additively, but in case they are mutually exclusive, for instance
in case of `tui-react` and `tui`, or `crossterm` and `termion`, the more general one will be chosen.

* _mutually exclusive_
    * **crossterm**
      * provides `Key` conversion support from `crossbeam::event::KeyEvent` and an `AlternativeRawTerminal`
      * provides a threaded key input channel
      * _additive_
        * _mutually exclusive_
            * **input-thread**
              * Adds input handling by spawning a thread providing input via `crossbeam` channels
            * **input-thread-flume**
              * Adds input handling by spawning a thread providing input via `flume` channels
        * **input-async**
          * adds native async capabilites to crossterm, which works without spawning an extra thread thanks to `mio`.
    * **termion**
      * provides `Key` conversion support from `termion::event::Key` and an `AlternativeRawTerminal`
      * provides a threaded key input channel
      * _additive_
        * _mutually exclusive_
            * **input-thread**
              * Adds input handling by spawning a thread providing input via `crossbeam` channels
            * **input-thread-flume**
              * Adds input handling by spawning a thread providing input via `flume` channels
        * **input-async**
          * Spawn a thread and provide input events via a futures Stream
* _mutually exclusive_
    * _using `tui_` _(mutually exclusive)_
        * **tui-termion** _implies `termion` feature_
          * combines `tui` with `termion` and provides a `tui::Terminal` with `termion` backend
        * **tui-crossterm**  _implies `crossterm` feature_
          * combines `tui` with `crossterm` and provides a `tui::Terminal` with `crossterm` backend
    * _using `tui-react`_ _(mutually exclusive)_
        * **tui-react-termion** _implies `termion` feature_
          * combines `tui-react` with `crossterm` and provides a `tui::Terminal` with `crossterm` backend
        * **tui-react-crossterm** _implies `crossterm` feature_
          * combines `tui-react` with `crossterm` and provides a `tui::Terminal` with `crossterm` backend
* **flume-async**
   * activates 'flume/async'
   * note that 'flume/select' is always active, as it is just a few lines of code without any additional dependencies
