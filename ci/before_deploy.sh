#!/bin/bash
# This script takes care of building your crate and packaging it for release
set -ex

try_compress() {
    local file=$1
    if [ $TRAVIS_OS_NAME = "osx" ]; then
        # https://stackoverflow.com/questions/56981572/how-to-update-objdump-got-unknown-command-line-argument-m
        # > objdump on a Mac is llvm-objdump, not GNU Binutils objdump
        stat $file
        strip $file || true
        objdump -file-headers $file || true
        stat $file
    else
        local header=$(objdump -f $file)
        if ! echo $header | grep -P "architecture: \s*UNKNOWN" ; then
            local target=$(echo $header| grep -oP "(?<=format )\s*[\w-]+" | tr -d '\n' || "")
            if [ -n "$target" ] ; then
                strip -v $file --target $target
                objdump -f $file || true
            fi
        fi
    fi
    if command -v upx ; then
        upx $file || true
    fi
}

main() {
    local src=$(pwd) \
          stage=

    case $TRAVIS_OS_NAME in
        linux)
            stage=$(mktemp -d)
            ;;
        osx)
            stage=$(mktemp -d -t tmp)
            ;;
    esac

    test -f Cargo.lock || cargo generate-lockfile

    cross rustc --bin $CRATE_NAME --target $TARGET --release -- -C lto
    
    if [ $TARGET = x86_64-pc-windows-gnu ]; then
        suffix=".exe"
    else
        suffix=""
    fi
    origin=target/$TARGET/release/$CRATE_NAME$suffix
    try_compress $origin
    cp $origin $src/$CRATE_NAME-$TARGET$suffix
    # cp target/$TARGET/release/$CRATE_NAME $stage/

    # cd $stage
    # tar czf $src/$CRATE_NAME-$TRAVIS_TAG-$TARGET.tar.gz *
    # cd $src

    # rm -rf $stage
}

main
