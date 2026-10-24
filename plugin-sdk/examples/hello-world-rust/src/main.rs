use tulpa_plugin::{CallContext, Level, Param, Plugin, ToolFailure, ToolSpec, Value, async_trait, json};

struct Hello;

#[async_trait]
impl Plugin for Hello {
    fn tools(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec::new("hello.greet", "Greets a person by name. Use it when the user asks for a greeting from the hello plugin.")
                .param(Param::string("name", "Who to greet").optional()),
        ]
    }

    async fn call_tool(&self, tool: &str, args: Value, ctx: CallContext) -> Result<Value, ToolFailure> {
        match tool {
            "hello.greet" => {
                let name = args["name"].as_str().filter(|n| !n.trim().is_empty()).unwrap_or("there");
                ctx.host.log(Level::Info, &format!("greeting {name} for user {}", ctx.user_id)).await;
                Ok(json!({"greeting": format!("Hello, {name}! (from the external hello plugin, for user {})", ctx.user_id)}))
            }
            other => Err(ToolFailure::not_found(format!("no such tool: {other}"))),
        }
    }
}

fn main() -> Result<(), tulpa_plugin::Error> {
    tulpa_plugin::serve(Hello)
}
