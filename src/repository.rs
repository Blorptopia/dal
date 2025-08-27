use uuid::Uuid;

pub(crate) struct Repository {
    pool: sqlx::PgPool
}
impl Repository {
    pub(crate) async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = sqlx::PgPool::connect(database_url).await?;

        Ok(Self {
            pool
        })
    }
    pub(crate) async fn has_sent_solve(&self, challenge_name: &str, player_id: &Uuid) -> Result<bool, sqlx::Error> {
        sqlx::query!(
                "
                    select
                        solve_id
                    from sent_solves
                    where
                        challenge_name = $1 and
                        player_id = $2
                    limit 1
                ",
                challenge_name,
                player_id 
            )
            .fetch_optional(&self.pool)
            .await
            .map(|maybe_solve_id| maybe_solve_id.is_some())
    }
    pub(crate) async fn has_been_solved(&self, challenge_name: &str) -> Result<bool, sqlx::Error> {
        sqlx::query!(
            "
                select
                    solve_id
                from sent_solves
                where
                    challenge_name = $1
                limit 1
            ",
            challenge_name
        )
        .fetch_optional(&self.pool)
        .await
        .map(|maybe_solve_id| maybe_solve_id.is_some())
    }
    pub(crate) async fn mark_challenge_as_solved(&self, challenge_name: &str, player_id: &Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "
                insert into sent_solves
                (challenge_name, player_id)
                values ($1, $2)
            ",
            challenge_name,
            player_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
