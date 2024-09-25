contract NotSuint {
    function test() public {
        uint256 a = 10;  // 1010 in binary
        uint256 b = 12;  // 1100 in binary

        assert(~suint256(a) == 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF5);
        assert(~a == 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF5);
    }
}