use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;
use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use tokio::time::timeout;

static DB: OnceLock<Mutex<HashMap<i64, String>>> = OnceLock::new();

fn get_db() -> &'static Mutex<HashMap<i64, String>> {
    DB.get_or_init(|| HashMap::new().into())
}

#[tokio::main]
async fn main() {
    env_logger::init();
    log::info!("start bot");

    let bot = Bot::from_env();
    Command::repl(bot, handle).await;
    log::info!("stop bot");
}

#[derive(Debug, Deserialize)]
struct DataUsageSummary {
    monthly_bw_limit_b: u64,
    bw_counter_b: u64,
    bw_reset_day_of_month: u64,
}

async fn handle_get_data_usage(url: &str) -> Result<String, String> {
    let summary = timeout(Duration::from_secs(5), async {
        let client = reqwest::Client::new();
        let resp = client.get(url).send().await?;
        let summary = resp.json::<DataUsageSummary>().await?;
        Ok::<_, anyhow::Error>(summary)
    })
    .await
    .map_err(|_| "获取流量信息超时")?
    .map_err(|e| format!("获取流量信息时发生错误：{}", e))?;

    let bw_counter_b_gb = summary.bw_counter_b as f64 / 1024.0 / 1024.0 / 1024.0;
    let monthly_bw_limit_b_gb = summary.monthly_bw_limit_b as f64 / 1024.0 / 1024.0 / 1024.0;
    let ratio_persent = 100.0 * bw_counter_b_gb / monthly_bw_limit_b_gb;
    Ok(format!(
        "已使用流量/总流量：{:.3}GiB / {:.3}GiB\n比例：{:.0}%\n每月流量重置日期：{}日",
        bw_counter_b_gb, monthly_bw_limit_b_gb, ratio_persent, summary.bw_reset_day_of_month
    ))
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "可利用的命令：")]
enum Command {
    #[command(description = "立即获取流量信息")]
    Usage(String),
    #[command(description = "依据历史URL立即获取流量信息")]
    Current,
}

async fn handle(bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    match cmd {
        Command::Usage(url) => {
            log::debug!("receive command usage");
            let message = match handle_get_data_usage(&url).await {
                Ok(usage_info) => {
                    get_db().lock().unwrap().insert(msg.chat.id.0, url);
                    usage_info
                }
                Err(e) => e,
            };
            bot.send_message(msg.chat.id, message).await?;
        }
        Command::Current => {
            log::debug!("receive command current");
            let Some(url) = get_db().lock().unwrap().get(&msg.chat.id.0).cloned() else {
                bot.send_message(msg.chat.id, "没有历史URL可用").await?;
                return Ok(());
            };
            let message = handle_get_data_usage(&url).await.unwrap_or_else(|e| e);
            bot.send_message(msg.chat.id, message).await?;
        }
    }
    Ok(())
}
