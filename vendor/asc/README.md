# asc

Files included in swfasm with the `asc-vendored` feature.

* `asc.jar`: the ActionScript 3 compiler from Apache Flex, licensed under the
  Apache License 2.0 (see `META-INF/LICENSE` and `META-INF/NOTICE` in the jar).
* `playerglobal.abc`: Ruffle's playerglobal, the definitions of the built-in
  classes that scripts are compiled against. It is licensed under the MIT
  License or the Apache License 2.0, like Ruffle. It was built from Ruffle
  commit `1d7cf3b682` with:

  ```sh
  cargo run --package=build_playerglobal -- compile <out_dir>
  ```

  and copied from `<out_dir>/playerglobal_import.abc`.
