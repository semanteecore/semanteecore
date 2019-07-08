#!/usr/bin/env bash


function usage() {
    echo "move_binaries.sh SOURCE DEST WITH_SUFFIX"
    echo ""
    echo "SOURCE       source path where binaries are now"
    echo "DEST         destination path where binaries should be moved"
    echo "WITH_SUFFIX  append suffix to each binary name"
    echo ""
}

SOURCE="$1"
DEST="$2"
WITH_SUFFIX="$3"

if [[ -z "${SOURCE}" ]]; then
    usage
    echo "Error: SOURCE must not be empty"
    exit 1
fi

if [[ -z "${DEST}" ]]; then
    usage
    echo "Error: DEST must not be empty"
    exit 1
fi

if [[ -z "${WITH_SUFFIX}" ]]; then
    echo "WARNING: WITH_SUFFIX is empty, artifacts may be overriten if pipeline generates multi-platform builds"
fi

SCRIPTPATH="$( cd "$(dirname "$0")" ; pwd -P )"

mkdir -p ${DEST}

for binary_name in $(${SCRIPTPATH}/list_cargo_binaries.py); do
    extension="${binary_name##*.}"
    filename="${binary_name%.*}"
    name_with_suffix=""
    if [[ ${extension} -eq "" ]]; then
        name_with_suffix="${filename}${WITH_SUFFIX}"
    else
        name_with_suffix="${filename}${WITH_SUFFIX}.${extension}"
    fi

    source="${SOURCE}/${binary_name}"
    dest="${DEST}/${name_with_suffix}"

    echo "${source} -> ${dest}" 1>&2
    mv ${source} ${dest}
done
