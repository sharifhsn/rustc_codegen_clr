typedef struct sqlite3 sqlite3;
typedef int (*sqlite3_callback)(void *context, int columns, char **values, char **names);

int sqlite3_open(const char *filename, sqlite3 **database);
int sqlite3_close(sqlite3 *database);
int sqlite3_exec(
    sqlite3 *database,
    const char *sql,
    sqlite3_callback callback,
    void *context,
    char **error_message
);
const char *sqlite3_errmsg(sqlite3 *database);
void sqlite3_free(void *pointer);
int sqlite3_libversion_number(void);
