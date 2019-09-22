#!/bin/bash
# This script takes care of building your crate and packaging it for release
set -ex

try_compress() {
    file=$1
    target=$(objdump -a $file | grep -oP "(?<=format )[\w-]+" | tr -d '\n' || "")
    if [ -n "$target" ] && strip -v $file --target $target; then
        echo "Stripped $file (target: $target)."
    fi
    if upx $file; then
        echo "Upx $file done."
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
