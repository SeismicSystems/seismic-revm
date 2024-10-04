contract NotSuint {
    function test() public pure {
        uint256 a = 10;  // 1010 in binary

        assert(~suint256(a) == 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF5);
        assert(~a == 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF5);
    }
}