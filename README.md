# A Simple Unity Cache Server Implementation in Rust

A Simple [Unity Cache Server](https://github.com/Unity-Technologies/unity-cache-server) Implementation in Rust.

## Feature

1. Listen on `0.0.0.0:8126`.
2. Files save to `.cache_fs`.

## Not support

1. Not support: stream hasher based high reliability mode (Only stored when two clients give same hash)
2. Not support: Expire time (30 days by default in official implementation)