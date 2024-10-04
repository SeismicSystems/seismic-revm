contract ExpSuint {
    function test() public pure {
        uint256 a = 10;
        uint256 b = 3;
        assert(suint256(a) ** suint256(b) == 1000);
        assert(suint256(a) ** b == 1000);
        assert(a ** suint256(b) == 1000);
        assert(a ** b == 1000);
    }
}