{
  lib,
  pkgs,
  source,
}:
let
  workspaceManifest = builtins.fromTOML (builtins.readFile (source + "/Cargo.toml"));
  manifest = workspaceManifest.workspace.metadata.neomacs-production-capabilities;
  knownCargoCapabilities = [
    "video"
    "webview"
  ];
  platform =
    if pkgs.stdenv.isLinux then
      "linux"
    else if pkgs.stdenv.isDarwin then
      "darwin"
    else
      throw "Neomacs has no production capability profile for ${pkgs.stdenv.hostPlatform.system}";
  profile = manifest.${platform};
  cargoFeatures = profile.cargo-features;
  videoBackend = profile.video-backend;
  unknownFeatures = lib.subtractLists knownCargoCapabilities cargoFeatures;
in
assert lib.assertMsg (
  manifest.schema-version == 1
) "unsupported Neomacs production capability schema";
assert lib.assertMsg (
  unknownFeatures == [ ]
) "unknown Neomacs production Cargo capabilities: ${lib.concatStringsSep ", " unknownFeatures}";
assert lib.assertMsg (builtins.elem videoBackend [
  "none"
  "linked-gstreamer"
]) "unknown Neomacs production video backend: ${videoBackend}";
assert lib.assertMsg
  (
    if platform == "linux" then
      videoBackend == "linked-gstreamer" && builtins.elem "video" cargoFeatures
    else
      videoBackend == "none" && !(builtins.elem "video" cargoFeatures)
  )
  "invalid Neomacs production video product for ${platform}: backend ${videoBackend}, features ${lib.concatStringsSep ", " cargoFeatures}";
{
  inherit cargoFeatures videoBackend;
}
