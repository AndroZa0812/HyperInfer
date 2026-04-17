use redis::AsyncCommands;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = redis::Client::open("redis://127.0.0.1/")?;
    let mut con = client.get_multiplexed_async_connection().await?;
    
    let _: () = redis::cmd("DEL").arg("mystream").query_async(&mut con).await?;
    let _: () = redis::cmd("XGROUP").arg("CREATE").arg("mystream").arg("mygroup").arg("0").arg("MKSTREAM").query_async(&mut con).await?;

    let result: Result<Vec<(String, Vec<(String, Vec<(String, String)>)>)>, _> = redis::cmd("XREADGROUP")
        .arg("GROUP")
        .arg("mygroup")
        .arg("myconsumer")
        .arg("COUNT")
        .arg(10)
        .arg("BLOCK")
        .arg(100)
        .arg("STREAMS")
        .arg("mystream")
        .arg(">")
        .query_async(&mut con)
        .await;
        
    println!("Result: {:?}", result);
    Ok(())
}
