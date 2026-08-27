#!/usr/bin/env bash
# Fetches the two Adobe resources this engine reads at run time.
#
# `external/` is not versioned, so without this script the data is whatever the last
# person happened to have. Both are BSD-licensed and redistributable; neither is vendored
# into the binary.
#
# **They are two repositories because they answer opposite questions**, and reading one
# for the other's job is how 7,617 CIDs came to be unreadable:
#
#   cmap-resources          Unicode -> CID. Its own README: CMap resources "unidirectionally
#                           map character codes ... to CIDs". This is what a document's
#                           /Encoding names.
#   mapping-resources-pdf   CID -> Unicode. This is what text extraction needs, and it is
#                           the only thing that answers it: `Adobe-Japan1-UCS2` names all
#                           23,060 CIDs of the collection, where a table derived from the
#                           first repository reached 15,443 and the 7,617 it missed were
#                           ordinary text — CID 12506 is の.
#
# Both are optional at run time. Without them a document whose fonts carry no /ToUnicode
# extracts nothing, and says so: `fepdf inspect text` records a 9.10.2 decision listing
# the directories it searched.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fetch() {
    local url=$1 dest=$2
    if [ -d "$dest" ]; then
        echo "  present: $dest"
        return
    fi
    echo "  fetching: $dest"
    git clone --depth 1 -q "$url" "$dest" || {
        echo "  FAILED: $url" >&2
        return 1
    }
}

mkdir -p external
echo "Adobe font resources:"
fetch https://github.com/adobe-type-tools/cmap-resources.git external/adobe-cmaps
fetch https://github.com/adobe-type-tools/mapping-resources-pdf.git external/mapping-resources-pdf

echo
echo "Where the engine looks for them, in order:"
echo "  \$FEPDF_RESOURCES/{cmaps,cid2unicode}   (exclusive when set)"
echo "  <exe>/../share/fepdf, <exe>/resources"
echo "  the user's data directory, then the system's"
echo "  this source tree, which is what the two paths above are"
