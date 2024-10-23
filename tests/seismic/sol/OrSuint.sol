contract OrSuint {
    function test() public pure {
        uint256 a = 10;  // 1010 in binary
        uint256 b = 12;  // 1100 in binary

        assert(suint256(a) | suint256(b) == 14);
        assert(suint256(a) | b == 14);
        assert(a | suint256(b) == 14);
        assert(a | b == 14);
    }
}