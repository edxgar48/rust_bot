use yew::prelude::*;

fn main() {
    yew::start_app::<App>();
}

struct App {
    link: ComponentLink<Self>,
    counter: i32,
}

impl Component for App {
    type Message = Msg;
    type Properties = ();

    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        App { link, counter: 0 }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Increment => {
                self.counter += 1;
                true
            }
        }
    }

    fn view(&self) -> Html {
        html! {
            <div>
                <button onclick=self.link.callback(|_| Msg::Increment)>{"Increment"}</button>
                <p>{"Counter: "}{self.counter}</p>
            </div>
        }
    }
}

enum Msg {
    Increment,
}