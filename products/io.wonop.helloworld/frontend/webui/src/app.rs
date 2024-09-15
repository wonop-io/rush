use yew::prelude::*;
use yew_router::prelude::*;
use gloo::net::http::Request;
use crate::routes::Route;
use api_types::{
  ExampleApiType, ApiResponse
};
use serde_json::from_str;

#[function_component(HomePage)]
pub fn home_page() -> Html {
    let state = use_state(|| ExampleApiType::default());

    {
        let state = state.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                let response_text = Request::get("/api/hello-world")
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap();
                let api_response: ApiResponse<ExampleApiType> = from_str(&response_text).unwrap();
                if let Some(data) = api_response.data {
                    state.set(data);
                }
            });
            || ()
        });
    }

    html! {
        <div class="text-zinc-700 flex flex-col items-center justify-center h-screen bg-gradient-to-r from-purple-400 via-pink-500 to-red-500">
            <div class="bg-white p-12 rounded-lg shadow-lg text-center">
                <h1 class="text-4xl font-bold mb-4">{"Welcome to the Demo Page"}</h1>
                <p class="text-xl mb-4">{"Data fetched from the API:"}</p>
                <span class="text-2xl text-gray-800">{state.payload.clone()}</span>
            </div>
        </div>
    }
}


#[function_component(App)]
pub fn app() -> Html {

  let render = move |routes| match routes {
    Route::HomePage => {
        html! {<HomePage />}
    }
};

  
    html! {
        <BrowserRouter>
            <div class="bg-gray-900 min-h-screen">
                <Switch<Route> render={render} />
            </div>
        </BrowserRouter>
    }
}
