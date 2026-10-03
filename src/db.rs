use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

pub async fn connect(db_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let opts = SqliteConnectOptions::from_str(db_url)?
        .create_if_missing(true)
        .foreign_keys(true);
    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await
}

pub async fn migrate(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let sql = include_str!("../migrations/0001_init.sql");
    sqlx::raw_sql(sql).execute(pool).await?;
    Ok(())
}

/// 演示种子数据：2 家参展商、入口/场馆/经理各 1 人、3 个展位。
pub async fn seed(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let n: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    if n.0 > 0 {
        return Ok(());
    }

    let users = [
        ("u-alpha", "甲方参展商-王展", "exhibitor"),
        ("u-beta", "乙方参展商-李博", "exhibitor"),
        ("u-gate", "入口闸口-赵检", "gatekeeper"),
        ("u-venue", "场馆布展-孙班", "venue"),
        ("u-mgr", "场馆经理-周总", "manager"),
    ];
    for (id, name, role) in users {
        sqlx::query("INSERT INTO users(id,name,role) VALUES(?1,?2,?3)")
            .bind(id)
            .bind(name)
            .bind(role)
            .execute(pool)
            .await?;
    }

    sqlx::query("INSERT INTO exhibitors(id,name,user_id) VALUES('e-alpha','甲方法人公司','u-alpha')")
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO exhibitors(id,name,user_id) VALUES('e-beta','乙方艺术机构','u-beta')")
        .execute(pool)
        .await?;

    sqlx::query(
        "INSERT INTO booths(id,code,name,owner_exhibitor_id) VALUES \
         ('b-a1','A1','A1 主展位','e-alpha'),\
         ('b-a2','A2','A2 副展位','e-alpha'),\
         ('b-b1','B1','B1 特装展位','e-beta')",
    )
    .execute(pool)
    .await?;

    Ok(())
}
