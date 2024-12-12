// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SEISMICRNG {
    function seismicRng() public view returns (bytes32 result) {
        address rngPrecompile = address(0x64);

        bytes memory input = bytes.concat(bytes1(0x00));

        // Call the precompile
        (bool success, bytes memory output) = rngPrecompile.staticcall(input);
        // Ensure the call was successful
        require(success, "RNG Precompile call failed");

        assembly {
            result := mload(add(output, 32))
        }
        
    }

    function seismicRngPers(bytes32 pers) public view returns (bytes32 result) {
        address rngPrecompile = address(0x64);

        bytes memory input = bytes.concat(pers);

        // Call the precompile
        (bool success, bytes memory output) = rngPrecompile.staticcall(input);
        
        // Ensure the call was successful
        require(success, "RNG Precompile call failed");

        assembly {
            result := mload(add(output, 32))
        }
        
    }

    function test_seismicRngPers() public view returns (bytes32 result) {
        bytes32 pers = 0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef;
        result = seismicRngPers(pers);
    }

}
// TODO: figure out how to test seismicRngPers() better, remove test_seismicRngPers()
// ====
// EVMVersion: >=mercury
// ====
// ----
// seismicRng() -> 0x825cc461d9bdde5725c73c657110151844251343ec047a9d1be7dd4af9288482
// seismicRng() ->  0x11703112df339aa0322e9c4d506178094adf3c624eb61ad94407575428ab3e37
// test_seismicRngPers() ->  0x6ec20a48cc3b4adc7c8a8c85b73698a6068868469d582d2b7711ada95baaa3b5