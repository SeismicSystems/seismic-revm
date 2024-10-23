contract RShiftSuint {
    function test() public pure {
        uint256 b = 12;  // 1100 in binary

        assert(suint256(b) >> 1 == 6);
        assert(b >> 1 == 6);
    }
}