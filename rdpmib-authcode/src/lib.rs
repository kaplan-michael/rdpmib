use std::io;

use tao::error::OsError;
use tao::event::Event;
use tao::event::WindowEvent;
use tao::event_loop::ControlFlow;
use tao::event_loop::EventLoopBuilder;
use tao::event_loop::EventLoopProxy;
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::platform::unix::WindowExtUnix;
use tao::window::WindowBuilder;
use thiserror::Error;
use url::ParseError;
use wry::PageLoadEvent;
use wry::WebContext;
use wry::WebView;
use wry::WebViewBuilder;
use wry::WebViewBuilderExtUnix;

#[derive(Debug, Error)]
pub enum GetAuthcodeError {
    #[error("{0}")]
    Os(#[from] OsError),

    #[error("{0}")]
    Wry(#[from] wry::Error),

    #[error("{0}")]
    UrlParse(#[from] ParseError),

    #[error("{0}")]
    Io(#[from] io::Error),

    #[error("canceled")]
    Canceled,

    #[error("{0}")]
    Failed(String),
}

#[derive(Debug, Clone)]
enum State {
    Begin,
    Login,
    GetCode,
    Done,
}

impl State {
    fn transition(&mut self, next: State, event_loop_proxy: &EventLoopProxy<UserEvent>) {
        *self = next;
        event_loop_proxy
            .send_event(UserEvent::StateChanged(self.clone()))
            .expect("event_loop_proxy must not closed");
    }
}

#[derive(Debug)]
enum UserEvent {
    PageLoading(String),
    StateChanged(State),
}

const LOGIN_URL: &'static str = "https://login.microsoftonline.com/";

fn handle_event(
    url: &url::Url,
    state: &mut State,
    webview: &WebView,
    event_loop_proxy: &EventLoopProxy<UserEvent>,
    event: Event<UserEvent>,
) -> Result<Option<String>, GetAuthcodeError> {
    match event {
        Event::WindowEvent {
            window_id: _,
            event: WindowEvent::CloseRequested,
            ..
        } => {
            return Err(GetAuthcodeError::Canceled);
        }

        Event::UserEvent(UserEvent::StateChanged(state)) => match state {
            State::Begin => {
                webview.load_url(LOGIN_URL)?;
            }
            State::Login => {}
            State::GetCode => {
                webview.load_url(&url.to_string())?;
            }
            State::Done => {}
        },

        Event::UserEvent(UserEvent::PageLoading(url)) => {
            let url = url::Url::parse(&url)?;

            match state {
                State::Begin => {
                    state.transition(State::Login, event_loop_proxy);
                }
                State::Login => {
                    let login_url_origin = url::Url::parse(LOGIN_URL).expect("must parse").origin();
                    let origin = url.origin();
                    if login_url_origin == origin {
                        return Ok(None);
                    }

                    state.transition(State::GetCode, event_loop_proxy);
                }
                State::GetCode => {
                    for (key, val) in url.query_pairs() {
                        match key.as_ref() {
                            "code" => {
                                state.transition(State::Done, event_loop_proxy);
                                return Ok(Some(val.to_string()))
                            }
                            "err" => {
                                state.transition(State::Done, event_loop_proxy);
                                return Err(GetAuthcodeError::Failed(val.to_string()));
                            }
                            _ => {}
                        }
                    }
                }
                State::Done => {}
            }
        }

        _ => {}
    }

    Ok(None)
}

pub fn get_authcode(url: &str) -> Result<String, GetAuthcodeError> {
    let url = url::Url::parse(url)?;

    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let event_proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title(env!("CARGO_PKG_NAME"))
        .build(&event_loop)?;
    let vbox = window.default_vbox().unwrap();

    let dirs = xdg::BaseDirectories::with_prefix("rdpmib");
    let dir = dirs.create_data_directory("webkit")?;
    let mut web_cx = WebContext::new(Some(dir));
    let webview = WebViewBuilder::new_with_web_context(&mut web_cx)
        .with_on_page_load_handler(move |event, url| {
            if !matches!(event, PageLoadEvent::Started) {
                return;
            }

            event_proxy.send_event(UserEvent::PageLoading(url)).unwrap();
        })
        .build_gtk(vbox)?;

    let mut state = State::Begin;
    let mut result: Option<Result<String, GetAuthcodeError>> = None;

    let event_loop_proxy = event_loop.create_proxy();
    event_loop_proxy
        .send_event(UserEvent::StateChanged(State::Begin))
        .expect("event_loop mut not closed");

    event_loop.run_return(|event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        result = Some(
            match handle_event(&url, &mut state, &webview, &event_loop_proxy, event) {
                Ok(None) => return,
                Ok(Some(val)) => Ok(val),
                Err(err) => Err(err),
            },
        );
        *control_flow = ControlFlow::Exit;
    });
    result.take().unwrap()
}
