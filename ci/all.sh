#!/bin/bash -xev
# Copyright (c) 2017-2019, Substratum LLC (https://substratum.net) and/or its affiliates. All rights reserved.
CI_DIR="$( cd "$( dirname "$0" )" && pwd )"
PARENT_DIR="$1"

"$CI_DIR"/format.sh

# Remove these two lines to slow down the build
which sccache || cargo install sccache || echo "Skipping sccache installation"  # Should do significant work only once
sccache --start-server || echo "sccache server already running"
export RUSTC_WRAPPER=sccache
# TODO remove -Aproc-macro-derive-resolution-fallback when they are promoted to errors
export RUSTFLAGS="-D warnings -Anon-snake-case -Aproc-macro-derive-resolution-fallback"

echo "*********************************************************************************************************"
echo "*********************************************************************************************************"
echo "***                                            TEST_UTILS HEAD                                        ***"
cd "$CI_DIR/../test_utils"
ci/all.sh
echo "***                                            TEST_UTILS TAIL                                        ***"
echo "*********************************************************************************************************"
echo "*********************************************************************************************************"
echo "***                                             SUB_LIB HEAD                                          ***"
cd "$CI_DIR/../sub_lib"
ci/all.sh
echo "***                                             SUB_LIB TAIL                                          ***"
echo "*********************************************************************************************************"
echo "*********************************************************************************************************"
echo "***                                           PROXY SERVER HEAD                                       ***"
cd "$CI_DIR/../proxy_server_lib"
ci/all.sh
echo "***                                           PROXY SERVER TAIL                                       ***"
echo "*********************************************************************************************************"
echo "*********************************************************************************************************"
echo "***                                           PROXY CLIENT HEAD                                       ***"
cd "$CI_DIR/../proxy_client_lib"
ci/all.sh
echo "***                                           PROXY CLIENT TAIL                                       ***"
echo "*********************************************************************************************************"
echo "*********************************************************************************************************"
echo "***                                               NODE HEAD                                           ***"
cd "$CI_DIR/../node"
ci/all.sh "$PARENT_DIR"
echo "***                                               NODE TAIL                                           ***"
echo "*********************************************************************************************************"
echo "*********************************************************************************************************"
echo "***                                           DNS UTILITY HEAD                                        ***"
cd "$CI_DIR/../dns_utility"
ci/all.sh "$PARENT_DIR"
echo "***                                           DNS UTILITY TAIL                                        ***"
echo "*********************************************************************************************************"
echo "*********************************************************************************************************"
echo "***                                             NODE UI HEAD                                          ***"
cd "$CI_DIR/../node-ui"
ci/all.sh
echo "***                                             NODE UI TAIL                                          ***"
echo "*********************************************************************************************************"
echo "*********************************************************************************************************"
