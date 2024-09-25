contract ModSuint {
    function test() public {
        uint256 a = 17;
        uint256 b = 5;
        assert(suint256(a) % suint256(b) == 2);
        assert(suint256(a) % b == 2);
        assert(a % suint256(b) == 2);
        assert(a % b == 2);
    }
}