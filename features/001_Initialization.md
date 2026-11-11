---
status: review
---
## Step 1 - Create project

Download and build all external crates:
- clap
- rusqlite
- eyre
- chrono

Run the program with an argument, and print that.
## Step 2 - Init DB

### AC:
- Create file for init a connection for the DB.
- The DB connection should be created just once. It can be at the beginnen of the program, or at the first query.
- For testing, we muss use `Connection::open_in_memory`.
- The db file should live in the root directory of the project.
## Step 3 - Create `vocabulary_entries` table

### Acceptance Criteria:
- `migrations/01_init.sql` is created and have the next tables:

| Name Colum       | Type    | Default value          | Nullable | possible values                                                                                            |
| ---------------- | ------- | ---------------------- | -------- | ---------------------------------------------------------------------------------------------------------- |
| id               | integer | yes - autoincrement    | not null |                                                                                                            |
| word_study       | string  | no                     | not null |                                                                                                            |
| word_translation | string  | no                     | not null |                                                                                                            |
| part_of_speech   | string  | no                     | not null | noun, verb, adjective, adverb, pronoun, article, preposition, conjunction, numeral, particle, interjection |
| gender           | string  | no                     | null     | Male, Female, Neutral                                                                                      |
| updated_at       | string  | yes - currect_timestap | not null | format **ISO 8601 in UTC**                                                                                 |
- Repository file is created with a function that insert many words at once, and fetch the words with the id param, no offset or limit
### Considerations:
- For now, we are not going to exclude any field at the fetching time. So we are going to do a generic methods for rusqlite::row  -> (validate that the values can be safetly converted to rust with the corresponding types) -> valid struct type to be used anywhere
- The `id` field in Rust is an i32
- `part_of_speech` is an `enum`, `gender`is a `Option<enum>` 


## Step 4  - Create `schema_migrations` table

### Acceptance Criteria:
- Please add in `migrations/01_init.sql` the creation of the next table:

| Column       | SQLite definition                         | Description                                                                              |
| ------------ | ----------------------------------------- | ---------------------------------------------------------------------------------------- |
| `version`    | `INTEGER PRIMARY KEY`                     | Numeric version from the migration filename. For example, `01_init.sql` has version `1`. |
| `name`       | `TEXT NOT NULL`                           | Human-readable migration name, such as `init`.                                           |
| `applied_at` | `TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP` | ISO 8601 in UTC timestamp when the migration was applied.                                |

- Every time the code is executed, verify it the actual db have the last file from `migrations` directory, if not execute them once by once, until the DB is completely updated.
- Create test that init an empty DB, and execute each file from `migrations` directory, created that dynamic, so we don't need to change this test, for every new migration file.
- Every time you execute a migration file for updating a DB, you muss insert to `schema_migrations` this update, this need to be ins a transaction, so even the insert for `schema_migrations` failed, the migration batch will not be apply.
- If the db.sql file does not exist, execute every file on migrations.
- Also add the config for adding all the .sql files inside `shcmea_migrations`, so this are in the exe release file.