# Changelog

## [0.4.0](https://github.com/OpenElevo/ElevoWorkspace/compare/workspace-sdk-v0.3.0...workspace-sdk-v0.4.0) (2026-05-18)


### Features

* **server:** JWKS 模块支持 ES256 (ECDSA P-256) 签名算法 ([fb58f62](https://github.com/OpenElevo/ElevoWorkspace/commit/fb58f622678336bf8e783d47733d8ff5650a9fa1))


### Bug Fixes

* **sdk-typescript:** add idle-timeout dead-connection detection in StorageProvider ([614426f](https://github.com/OpenElevo/ElevoWorkspace/commit/614426f8bbfcdb0af2e6dc41bda36089029d7047))
* **sdk:** make fileWatcher.start() fire-and-forget to prevent handshake deadlock ([82c2c57](https://github.com/OpenElevo/ElevoWorkspace/commit/82c2c57e062193627965df25571110da4b4a726f))

## [0.3.0](https://github.com/OpenElevo/ElevoWorkspace/compare/workspace-sdk-v0.2.0...workspace-sdk-v0.3.0) (2026-05-13)


### Features

* **sdk-typescript:** add StorageProvider and fix FUSE mount deadlock ([a34107b](https://github.com/OpenElevo/ElevoWorkspace/commit/a34107b20198bbc4e31371f705c8e0c2f1e83c80))
* **sdk-typescript:** expose allowOther, allowRoot, readOnly options in FuseService ([50eda68](https://github.com/OpenElevo/ElevoWorkspace/commit/50eda68a0a2c77193da25a340c2558dc9bc85376))
* **sdk:** add full SDK test examples and FUSE improvements ([cbd177c](https://github.com/OpenElevo/ElevoWorkspace/commit/cbd177c5d270f9d66cb750e60b654e6b6c624da8))
* **sdk:** add FUSE mount service for all SDKs ([48467d0](https://github.com/OpenElevo/ElevoWorkspace/commit/48467d09b2edc74a00b86e145cd77305b526ac12))
* **sdk:** add NFS mount capability to all SDKs ([d6fb5d6](https://github.com/OpenElevo/ElevoWorkspace/commit/d6fb5d6acd39596343285278bb66f1d8d29fda08))
* **sdk:** add server-first binary download with GitHub fallback ([4f08750](https://github.com/OpenElevo/ElevoWorkspace/commit/4f08750ed438b97bda84dd7a6c9f4476e6052402))
* **sdk:** make FUSE token optional in all SDKs ([5c53f6e](https://github.com/OpenElevo/ElevoWorkspace/commit/5c53f6e9db1399952b8590442a57079c30933ff9))
* **sdk:** migrate all SDKs from HTTP to gRPC ([39992bb](https://github.com/OpenElevo/ElevoWorkspace/commit/39992bb623b301ce6248c7db9551210b4488c662))
* **sdk:** synchronize TypeScript & Python SDKs with Go SDK ([34de546](https://github.com/OpenElevo/ElevoWorkspace/commit/34de54656bf022f92cefbd8e0e96ea385bc90e8b))
* **sdk:** update Go, Python, TypeScript SDKs for namespace/share model ([24371c4](https://github.com/OpenElevo/ElevoWorkspace/commit/24371c4638b6549912eaec6f4df1c386ddc7d0f5))
* **server:** JWKS 模块支持 ES256 (ECDSA P-256) 签名算法 ([fb58f62](https://github.com/OpenElevo/ElevoWorkspace/commit/fb58f622678336bf8e783d47733d8ff5650a9fa1))


### Bug Fixes

* **ci:** fix go vet warnings and rust formatting ([f4b74cd](https://github.com/OpenElevo/ElevoWorkspace/commit/f4b74cd758dcfb3d4ff06a392999b40277302d8c))
* **sdk-typescript:** add CommonJS support for Jest compatibility ([5f3d508](https://github.com/OpenElevo/ElevoWorkspace/commit/5f3d508b3dd0b118e8b8f666bb46fe2f54997107))
* **sdk-typescript:** add idle-timeout dead-connection detection in StorageProvider ([614426f](https://github.com/OpenElevo/ElevoWorkspace/commit/614426f8bbfcdb0af2e6dc41bda36089029d7047))
* **sdk-typescript:** esm ([4f7a6b1](https://github.com/OpenElevo/ElevoWorkspace/commit/4f7a6b1460eec0853ee62dfa97ee26469dcf9f94))
* **sdk-typescript:** esm ([255c076](https://github.com/OpenElevo/ElevoWorkspace/commit/255c076cfabae34ca5a4366bc5d68c7530c2c7bf))
* **sdk-typescript:** rename npm scope to openelevo ([430998a](https://github.com/OpenElevo/ElevoWorkspace/commit/430998a4651d4a9ae43a363b0564190cf414c012))
* **sdk-typescript:** resolve duplicate type/error definitions after rebase ([4ceefa1](https://github.com/OpenElevo/ElevoWorkspace/commit/4ceefa1a33528847f8b7ea4f39c442a9de3a5112))
* **sdk:** remove hardcoded port conversion and fix GitHub download URL ([ffdeada](https://github.com/OpenElevo/ElevoWorkspace/commit/ffdeada32d08d94ab48b270d5a525fa35d9759ec))
* **sdk:** 修复 proto 路径解析并添加 proto 到 npm 发布包 ([dc97746](https://github.com/OpenElevo/ElevoWorkspace/commit/dc977463864f6a5f45c121bdfe60938115316abe))

## [0.2.0](https://github.com/OpenElevo/ElevoWorkspace/compare/workspace-sdk-v0.1.0...workspace-sdk-v0.2.0) (2026-01-22)


### Features

* **sdk:** add NFS mount capability to all SDKs ([d6fb5d6](https://github.com/OpenElevo/ElevoWorkspace/commit/d6fb5d6acd39596343285278bb66f1d8d29fda08))


### Bug Fixes

* **sdk-typescript:** esm ([cbb6290](https://github.com/OpenElevo/ElevoWorkspace/commit/cbb6290063c9dfe4772de11eef4e61827784457a))
* **sdk-typescript:** esm ([255c076](https://github.com/OpenElevo/ElevoWorkspace/commit/255c076cfabae34ca5a4366bc5d68c7530c2c7bf))
* **sdk-typescript:** rename npm scope to openelevo ([430998a](https://github.com/OpenElevo/ElevoWorkspace/commit/430998a4651d4a9ae43a363b0564190cf414c012))
