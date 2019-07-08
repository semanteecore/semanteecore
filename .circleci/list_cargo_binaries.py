#!/usr/bin/env python3

import json
import subprocess

### This tool uses cargo metadata inspection in order to
### detect all binary targets defined for the current project

# Read the manigest path
def read_cargo_manifest():
    result = subprocess.run(['cargo', 'metadata', '--no-deps', '--format-version=1'], stdout=subprocess.PIPE)
    cargo_manifest_txt = result.stdout.decode('utf-8')
    return json.loads(cargo_manifest_txt, encoding='utf-8')


def get_target_dir(manifest):
    return manifest['target_directory']


# Checks if project is a binary
def is_binary(target):
    return 'bin' in target['kind']


def list_targets(manifest):
    targets = []
    for package in manifest['packages']:
        for target in package['targets']:
            is_bin = is_binary(target)
            name = str(target['name'])
            targets.append({
                "binary": is_bin,
                "name": name
            })
    return targets


def list_binary_targets():
    manifest = read_cargo_manifest()
    targets = list_targets(manifest)
    output = []
    for target in targets:
        if target["binary"]:
            output.append(target["name"])
    return output


if __name__ == "__main__":
    for target in list_binary_targets():
        print(target, end=' ')


