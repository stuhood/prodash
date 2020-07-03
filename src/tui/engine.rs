use crate::{tree::Root, tui::draw, tui::ticker};

use futures_util::{stream, StreamExt};
use std::{
    io::{self, Write},
    time::Duration,
};
use tui::layout::Rect;

/// Configure the terminal user interface
#[derive(Clone)]
pub struct TuiOptions {
    /// The initial title to show for the whole window.
    ///
    /// Can be adjusted later by sending `Event::SetTitle(…)`
    /// into the event stream, see see [`tui::render_with_input(…events)`](./fn.render_with_input.html) function.
    pub title: String,
    /// The amount of frames to draw per second. If below 1.0, it determines the amount of seconds between the frame.
    ///
    /// *e.g.* 1.0/4.0 is one frame every 4 seconds.
    pub frames_per_second: f32,
    /// If set, recompute the column width of the task tree only every given frame. Otherwise the width will be recomputed every frame.
    ///
    /// Use this if there are many short-running tasks with varying names paired with high refresh rates of multiple frames per second to
    /// stabilize the appearance of the TUI.
    ///
    /// For example, setting the value to 40 will with a frame rate of 20 per second will recompute the column width to fit all task names
    /// every 2 seconds.
    pub recompute_column_width_every_nth_frame: Option<usize>,
    /// The initial window size.
    ///
    /// If unset, it will be retrieved from the current terminal.
    pub window_size: Option<Rect>,

    /// If true (default: false), we will skip potentially expensive redraws if nothing would change. This doubles the amount of memory.
    ///
    /// This is particularly useful if most of the time, the actual change rate is lower than the refresh rate. Drawing is expensive.
    pub redraw_only_on_state_change: bool,

    /// If true (default: false), we will stop running the TUI once there the list of drawable progress items is empty.
    ///
    /// Please note that you should add at least one item to the `prodash::Tree` before launching the application or else
    /// risk a race causing the TUI to sometimes not come up at all.
    pub stop_if_empty_progress: bool,
}

impl Default for TuiOptions {
    fn default() -> Self {
        TuiOptions {
            title: "Progress Dashboard".into(),
            frames_per_second: 10.0,
            recompute_column_width_every_nth_frame: None,
            window_size: None,
            redraw_only_on_state_change: false,
            stop_if_empty_progress: false,
        }
    }
}

/// A line as used in [`Event::SetInformation`](./enum.Event.html#variant.SetInformation)
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Line {
    /// Set a title with the given text
    Title(String),
    /// Set a line of text with the given content
    Text(String),
}

/// The variants represented here allow the user to control when the GUI can be shutdown.
#[derive(Debug, Clone, Copy)]
pub enum Interrupt {
    /// Immediately exit the GUI event loop when there is an interrupt request.
    ///
    /// This is the default when the event loop is entered.
    Instantly,
    /// Instead of exiting the event loop instantly, wait until the next Interrupt::Instantly
    /// event is coming in.
    Deferred,
}

#[derive(Clone, Copy)]
pub(crate) enum InterruptDrawInfo {
    Instantly,
    /// Boolean signals if interrupt is requested
    Deferred(bool),
}

pub mod input {
    use crossterm::event::KeyEvent;

    /// A set of possible key presses, equivalent to the one in `termion@1.5.5::event::Key`
    #[derive(Debug, Clone, Copy)]
    pub enum Key {
        Backspace,
        Left,
        Right,
        Up,
        Down,
        Home,
        End,
        PageUp,
        PageDown,
        BackTab,
        Delete,
        Insert,
        F(u8),
        Char(char),
        Alt(char),
        Ctrl(char),
        Null,
        Esc,
    }

    #[cfg(feature = "termion")]
    impl std::convert::TryFrom<termion::event::Key> for Key {
        type Error = termion::event::Key;

        fn try_from(value: termion::event::Key) -> Result<Self, Self::Error> {
            use termion::event::Key::*;
            Ok(match value {
                Backspace => Key::Backspace,
                Left => Key::Left,
                Right => Key::Right,
                Up => Key::Up,
                Down => Key::Down,
                Home => Key::Home,
                End => Key::End,
                PageUp => Key::PageUp,
                PageDown => Key::PageDown,
                BackTab => Key::BackTab,
                Delete => Key::Delete,
                Insert => Key::Insert,
                F(c) => Key::F(c),
                Char(c) => Key::Char(c),
                Alt(c) => Key::Alt(c),
                Ctrl(c) => Key::Ctrl(c),
                Null => Key::Null,
                Esc => Key::Esc,
                _ => return Err(value),
            })
        }
    }
    #[cfg(feature = "crossterm")]
    impl std::convert::TryFrom<crossterm::event::KeyEvent> for Key {
        type Error = crossterm::event::KeyEvent;

        fn try_from(value: KeyEvent) -> Result<Self, Self::Error> {
            use crossterm::event::{KeyCode::*, KeyModifiers};
            Ok(match value.code {
                Backspace => Key::Backspace,
                Enter => Key::Char('\n'),
                Left => Key::Left,
                Right => Key::Right,
                Up => Key::Up,
                Down => Key::Down,
                Home => Key::Home,
                End => Key::End,
                PageUp => Key::PageUp,
                PageDown => Key::PageDown,
                Tab => Key::Char('\t'),
                BackTab => Key::BackTab,
                Delete => Key::Delete,
                Insert => Key::Insert,
                F(k) => Key::F(k),
                Null => Key::Null,
                Esc => Key::Esc,
                Char(c) => match value.modifiers {
                    KeyModifiers::SHIFT => Key::Char(c),
                    KeyModifiers::CONTROL => Key::Ctrl(c),
                    KeyModifiers::ALT => Key::Alt(c),
                    _ => return Err(value),
                },
            })
        }
    }
}

use input::Key;

#[cfg(feature = "termion")]
mod _impl {
    use crate::tui::input::Key;
    use futures_util::SinkExt;
    use std::{convert::TryInto, io};
    use termion::{
        input::TermRead,
        raw::{IntoRawMode, RawTerminal},
        screen::AlternateScreen,
    };
    use tui::backend::TermionBackend;
    use tui_react::Terminal;

    pub fn new_terminal(
    ) -> Result<Terminal<TermionBackend<AlternateScreen<RawTerminal<io::Stdout>>>>, io::Error> {
        let stdout = io::stdout().into_raw_mode()?;
        let backend = TermionBackend::new(AlternateScreen::from(stdout));
        Ok(Terminal::new(backend)?)
    }

    pub fn key_input_stream() -> futures_channel::mpsc::Receiver<Key> {
        let (mut key_send, key_receive) = futures_channel::mpsc::channel::<Key>(1);
        // This brings blocking key-handling into the async world
        std::thread::spawn(move || -> Result<(), io::Error> {
            for key in io::stdin().keys() {
                let key: Result<Key, _> = key?.try_into();
                if let Ok(key) = key {
                    smol::block_on(key_send.send(key)).ok();
                }
            }
            Ok(())
        });
        key_receive
    }
}

#[cfg(not(any(feature = "termion", feature = "crossterm")))]
mod _impl {
    use crate::tui::engine::input::Key;
    use std::io;
    use tui::backend::TestBackend;
    use tui_react::Terminal;

    pub fn key_input_stream() -> futures_channel::mpsc::Receiver<Key> {
        unimplemented!("use either the 'termion' or the 'crossterm' feature");
    }

    pub fn new_terminal() -> Result<Terminal<TestBackend>, io::Error> {
        Terminal::new(TestBackend::new(100, 100))
    }
}

use _impl::{key_input_stream, new_terminal};

/// An event to be sent in the [`tui::render_with_input(…events)`](./fn.render_with_input.html) stream.
///
/// This way, the TUI can be instructed to draw frames or change the information to be displayed.
#[derive(Debug, Clone)]
pub enum Event {
    /// Draw a frame
    Tick,
    /// Send any key - can be used to simulate user input, and is typically generated by the TUI's own input loop.
    Input(Key),
    /// Change the size of the window to the given rectangle.
    ///
    /// Useful to embed the TUI into other terminal user interfaces that can resize dynamically.
    SetWindowSize(Rect),
    /// Set the title of the progress dashboard
    SetTitle(String),
    /// Provide a list of titles and lines to populate the side bar on the right.
    SetInformation(Vec<Line>),
    /// The way the GUI will respond to interrupt requests. See `Interrupt` for more information.
    SetInterruptMode(Interrupt),
}

/// Returns a future that draws the terminal user interface indefinitely.
///
/// * `progress` is the progress tree whose information to visualize.
///    It will usually be changing constantly while the TUI holds it.
/// * `options` are configuring the TUI.
/// * `events` is a stream of `Event`s which manipulate the TUI while it is running
///
/// Failure may occour if there is no terminal to draw into.
pub fn render_with_input(
    progress: Root,
    options: TuiOptions,
    events: impl futures_core::Stream<Item = Event> + Send,
) -> Result<impl std::future::Future<Output = ()>, std::io::Error> {
    let TuiOptions {
        title,
        frames_per_second,
        window_size,
        recompute_column_width_every_nth_frame,
        redraw_only_on_state_change,
        stop_if_empty_progress,
    } = options;
    let mut terminal = new_terminal()?;
    terminal.hide_cursor()?;

    let duration_per_frame = Duration::from_secs_f32(1.0 / frames_per_second);
    let key_receive = key_input_stream();

    let render_fut = async move {
        let mut state = draw::State {
            title,
            duration_per_frame,
            ..draw::State::default()
        };
        let mut interrupt_mode = InterruptDrawInfo::Instantly;
        let mut entries = Vec::with_capacity(progress.num_tasks());
        let mut messages = Vec::with_capacity(progress.messages_capacity());
        let mut events = stream::select_all(vec![
            ticker(duration_per_frame).map(|_| Event::Tick).boxed(),
            key_receive.map(|key| Event::Input(key)).boxed(),
            events.boxed(),
        ]);

        let mut tick = 0usize;
        let store_task_size_every = recompute_column_width_every_nth_frame.unwrap_or(1).max(1);
        let mut previous_root = None::<Root>;
        let mut previous_state = None::<draw::State>;
        while let Some(event) = events.next().await {
            let mut skip_redraw = false;
            match event {
                Event::Tick => {}
                Event::Input(key) => match key {
                    Key::Esc | Key::Char('q') | Key::Ctrl('c') | Key::Ctrl('[') => {
                        match interrupt_mode {
                            InterruptDrawInfo::Instantly => break,
                            InterruptDrawInfo::Deferred(_) => {
                                interrupt_mode = InterruptDrawInfo::Deferred(true)
                            }
                        }
                    }
                    Key::Char('`') => state.hide_messages = !state.hide_messages,
                    Key::Char('~') => state.messages_fullscreen = !state.messages_fullscreen,
                    Key::Char('J') => state.message_offset = state.message_offset.saturating_add(1),
                    Key::Char('D') => {
                        state.message_offset = state.message_offset.saturating_add(10)
                    }
                    Key::Char('j') => state.task_offset = state.task_offset.saturating_add(1),
                    Key::Char('d') => state.task_offset = state.task_offset.saturating_add(10),
                    Key::Char('K') => state.message_offset = state.message_offset.saturating_sub(1),
                    Key::Char('U') => {
                        state.message_offset = state.message_offset.saturating_sub(10)
                    }
                    Key::Char('k') => state.task_offset = state.task_offset.saturating_sub(1),
                    Key::Char('u') => state.task_offset = state.task_offset.saturating_sub(10),
                    Key::Char('[') => state.hide_info = !state.hide_info,
                    Key::Char('{') => state.maximize_info = !state.maximize_info,
                    _ => skip_redraw = true,
                },
                Event::SetWindowSize(bound) => state.user_provided_window_size = Some(bound),
                Event::SetTitle(title) => state.title = title,
                Event::SetInformation(info) => state.information = info,
                Event::SetInterruptMode(mode) => {
                    interrupt_mode = match mode {
                        Interrupt::Instantly => {
                            if let InterruptDrawInfo::Deferred(true) = interrupt_mode {
                                break;
                            }
                            InterruptDrawInfo::Instantly
                        }
                        Interrupt::Deferred => InterruptDrawInfo::Deferred(match interrupt_mode {
                            InterruptDrawInfo::Deferred(interrupt_requested) => interrupt_requested,
                            _ => false,
                        }),
                    };
                }
            }
            if !skip_redraw && redraw_only_on_state_change {
                let (new_prev_state, state_changed) = match previous_state.take() {
                    Some(prev) if prev == state => (Some(prev), false),
                    None | Some(_) => (Some(state.clone()), true),
                };
                previous_state = new_prev_state;
                if !state_changed {
                    previous_root = match previous_root.take() {
                        Some(prev) if prev.deep_eq(&progress) => {
                            skip_redraw = true;
                            Some(prev)
                        }
                        None | Some(_) => Some(progress.deep_clone()),
                    };
                }
            }
            if !skip_redraw {
                tick += 1;

                progress.sorted_snapshot(&mut entries);
                if stop_if_empty_progress && entries.is_empty() {
                    break;
                }
                let terminal_window_size = terminal.pre_render().expect("pre-render to work");
                let window_size = state
                    .user_provided_window_size
                    .or(window_size)
                    .unwrap_or(terminal_window_size);
                let buf = terminal.current_buffer_mut();
                if !state.hide_messages {
                    progress.copy_messages(&mut messages);
                }

                draw::all(
                    &mut state,
                    interrupt_mode,
                    &entries,
                    &messages,
                    window_size,
                    buf,
                );
                if tick == 1
                    || tick % store_task_size_every == 0
                    || state.last_tree_column_width.unwrap_or(0) == 0
                {
                    state.next_tree_column_width = state.last_tree_column_width;
                }
                terminal.post_render().expect("post render to work");
            }
        }
        // Make sure the terminal responds right away when this future stops, to reset back to the 'non-alternate' buffer
        drop(terminal);
        io::stdout().flush().ok();
    };
    Ok(render_fut)
}

/// An easy-to-use version of `render_with_input(…)` that does not allow state manipulation via an event stream.
pub fn render(
    progress: Root,
    config: TuiOptions,
) -> Result<impl std::future::Future<Output = ()>, std::io::Error> {
    return render_with_input(progress, config, stream::pending());
}
