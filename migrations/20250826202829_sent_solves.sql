create table sent_solves (
	solve_id serial primary key, -- Yes I know serial is bad but I want this done quick
	challenge_name text not null,
	player_id uuid not null
);
-- Bad name but I am again lazy.
create index sent_solves_lookup on sent_solves(challenge_name, player_id);
