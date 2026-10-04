## Project conventions

- Use `just fmt` to format code
- Use `just ci` as validation gate (linters, tests)

Use unit tests for small utility functions, and integration tests for composable engine parts. Prefer a small number of good integration tests.

Keep domain boundaries clean and separated, suggest improvements in architecture when different domains leak. 

Unless strictly necessary, functions should accepted borrowed values and return owned values.
