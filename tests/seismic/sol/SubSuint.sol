contract SubSuint {
    function test() public {
        uint256 a = 20;
        uint256 b = 3;
        assert(suint256(a) - suint256(b) == 17);
        assert(suint256(a) - b == 17);
        assert(a - suint256(b) == 17);
        assert(a - b == 17);
    }
}